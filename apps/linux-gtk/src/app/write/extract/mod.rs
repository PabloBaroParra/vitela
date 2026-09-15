//! Extract: pulling a subset of the open document's pages out into a new PDF.
//!
//! The Organize screen's "Extract" button lands here. It is a write chain —
//! it asks for a destination, guards the overwrite, and puts PDF bytes on
//! disk through the same [`chooser`](super::chooser) and
//! [`worker`](super::worker) the other three use — with one deliberate
//! difference: **nothing is installed afterwards**. Save, Sign and Protect all
//! mean "this file is now the document you are editing", and each folds its
//! result back into the session. An extraction means the opposite. The open
//! document is untouched, the user stays on the Organize screen looking at
//! the same pages, and a new file appears somewhere else.
//!
//! ## The permission gate
//!
//! [`Viewer::text_extraction_refusal`] — `/P` bit 5, whose text in PDF 1.7
//! table 22 is "copy or otherwise extract text **and graphics** from the
//! document". Lifting a page's content into another file is exactly that, and
//! this repository has already settled the question in that direction twice:
//! `pdf_manip::open_import_source_from_bytes` gates an *import* on the same
//! bit (`docs/batch-pdf-assembly.md`, "Progreso de las fuentes cifradas"),
//! and `export` gates a page raster on it. The bit's name says *text* because
//! text extraction was its first caller, not because it is narrower than the
//! operation.
//!
//! Deliberately **not** `page_assembly_refusal`. Bit 11 governs assembling
//! *this* document — inserting, rotating or deleting its pages — and an
//! extraction changes nothing about the open document at all. Asking it would
//! invent a restriction on a file that granted copying and withheld assembly,
//! which is a perfectly ordinary combination.
//!
//! [`Viewer::full_rewrite_refusal`] is asked as well, and for a reason that is
//! not about permission: a new file with a different page set can only come
//! from the full-rewrite writer, and an encrypted document opened with one of
//! its two passwords cannot be re-encrypted at all. Asked here rather than
//! discovered by the worker, so the user meets it before choosing a
//! destination rather than after.
//!
//! ## How this splits
//!
//! The cut `export` makes, for the same reason. This file owns the chain —
//! the gates, the request, the destination. [`options`] owns the rules the
//! dialog enforces, as plain functions with no widget in sight. [`dialog`]
//! owns the widgets that ask. [`worker`] owns everything after the
//! destination is settled.

mod dialog;
mod options;
mod worker;

use std::path::PathBuf;
use std::rc::Rc;

use gtk::ApplicationWindow;
use pdf_document::Document;

use super::chooser::choose_destination;
use super::EXTRACT;
use crate::app::state::{ImportedSource, SaveBacking, Viewer};

/// The snapshot an extraction carries from the live session onto the worker
/// thread.
///
/// Every field is owned: the worker outlives the borrow it was built from,
/// and `document` is a **clone** of the session's model precisely so that
/// pruning it to the chosen pages cannot touch what the user is still
/// editing — including their undo history, which the pruning writes to.
struct ExtractRequest {
    document: Document,
    backing: SaveBacking,
    sources: Vec<ImportedSource>,
    /// Zero-based page indices, ascending and deduplicated.
    pages: Vec<u32>,
    /// Generation of the session this extraction started under.
    /// [`worker::spawn_extract`] compares it before touching the status line,
    /// so one that finishes after the document was replaced says nothing.
    generation: u64,
    /// Whether the document this was extracted from carries a signature — the
    /// one thing the completion message has to add. See
    /// [`options::extract_summary`] for why it is a sentence afterwards
    /// rather than a modal before.
    signed: bool,
}

/// Opens the Extract dialog, or says why it cannot.
pub(crate) fn begin_extract(window: &ApplicationWindow, viewer: &Viewer) {
    if let Some(refusal) = viewer.text_extraction_refusal() {
        viewer.status.set_text(refusal);
        return;
    }
    if let Some(refusal) = viewer.full_rewrite_refusal() {
        viewer.status.set_text(refusal);
        return;
    }

    let Some(total_pages) = ({
        let state = viewer.state.borrow();
        state
            .session
            .as_ref()
            .and_then(|session| session.document_model.as_ref())
            .map(|document| document.pages.len() as u32)
    }) else {
        viewer
            .status
            .set_text("Open a PDF before extracting pages from it.");
        return;
    };
    if total_pages == 0 {
        viewer
            .status
            .set_text("This document has no pages to extract.");
        return;
    }

    dialog::prompt_for_pages(
        window,
        total_pages,
        {
            let viewer = viewer.clone();
            move || viewer.status.set_text(EXTRACT.cancelled)
        },
        {
            let viewer = viewer.clone();
            move |window, pages| {
                let viewer = viewer.clone();
                choose_destination(
                    window,
                    &viewer.clone(),
                    EXTRACT,
                    Rc::new(move |_window, viewer, destination| {
                        extract_to(viewer, pages.clone(), destination);
                    }),
                );
            }
        },
    );
}

/// Bundles what the worker needs out of the live session and hands it over,
/// or reports why the session can no longer answer for it.
///
/// Read under one borrow and cloned out of it, the way `save::save_current_to`
/// does: the dialog and the file chooser are both asynchronous, so everything
/// this reads may have been replaced since the user pressed Extract.
fn extract_to(viewer: &Viewer, pages: Vec<u32>, destination: PathBuf) {
    let request = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            viewer
                .status
                .set_text("Open a PDF before extracting pages from it.");
            return;
        };
        let Some(document) = session.document_model.clone() else {
            viewer
                .status
                .set_text("This document cannot be extracted from as an editable PDF.");
            return;
        };
        let Some(backing) = session.save_backing.clone() else {
            viewer.status.set_text("This document has no save backing.");
            return;
        };
        // The selection was parsed against the page count the dialog opened
        // on. A delete landing while the chooser was up would leave it naming
        // a page that is gone, and `Command::remove_pages` would refuse
        // mid-prune with a message about runs rather than about pages.
        if pages
            .iter()
            .any(|&page| page as usize >= document.pages.len())
        {
            viewer.status.set_text(
                "The document changed while the destination was being chosen. Try again.",
            );
            return;
        }
        ExtractRequest {
            signed: pdf_manip::document_has_signatures(&backing.base),
            document,
            backing,
            sources: session.imported_sources.clone(),
            pages,
            generation: state.generation,
        }
    };

    worker::spawn_extract(viewer, request, destination);
}

#[cfg(test)]
mod tests {
    use gtk::prelude::*;
    use pdf_document::{Document, Orientation, Page, PageId, PageSize};

    use super::begin_extract;
    use crate::app::state::TextAccess;
    use crate::app::test_fixtures::model_session;
    use crate::app::ui_tests::built_ui;

    /// An encrypted document opened the only way this shell can open one:
    /// with a single password. The second cannot be derived from the first,
    /// so no full rewrite of it can reproduce its encryption — and a new file
    /// holding a different page set is nothing but a full rewrite.
    fn one_password_security() -> pdf_document::SecurityContext {
        pdf_document::SecurityContext {
            handler: pdf_document::SecurityHandler::Aes128,
            credential: pdf_document::Credential::User,
            credentials: pdf_document::EncryptionCredentials::user("only-the-user-password"),
            permissions: pdf_document::Permissions(0xFFFF_FFFC),
        }
    }

    fn a_document_of(pages: u32) -> Document {
        let mut document = Document::blank();
        document.pages = (0..pages)
            .map(|id| Page::blank(PageId(id), PageSize::A4, Orientation::Portrait))
            .collect();
        document
    }

    /// The gate a cold start hits. The path through `begin_extract` must say
    /// so rather than open a dialog offering to extract from nothing.
    #[gtk::test]
    fn gtk_ui_extracting_without_an_open_document_is_refused() {
        let built = built_ui();

        begin_extract(&built.window, &built.viewer);

        assert_eq!(
            built.viewer.status.text().as_str(),
            "Open a PDF before extracting pages from it."
        );

        built.window.close();
    }

    /// `/P` bit 5 withheld: refused with the same sentence search, the
    /// selection clipboard and the image export use, before a dialog exists.
    #[gtk::test]
    fn gtk_ui_a_document_that_forbids_extraction_has_no_pages_pulled_out_of_it() {
        let built = built_ui();
        let mut session = model_session(a_document_of(3));
        session.text_access = TextAccess::Forbidden;
        built.viewer.state.borrow_mut().session = Some(session);

        begin_extract(&built.window, &built.viewer);

        assert_eq!(
            built.viewer.status.text().as_str(),
            TextAccess::Forbidden
                .refusal()
                .expect("a forbidden document refuses")
        );

        built.window.close();
    }

    /// The second gate, and a different question: this document permits
    /// copying, but it was opened with one of its two passwords, so the
    /// rewrite an extraction needs could never reproduce its encryption.
    /// Asked before the dialog rather than discovered by the worker.
    #[gtk::test]
    fn gtk_ui_a_document_that_cannot_be_rewritten_is_refused_before_the_dialog() {
        let built = built_ui();
        let mut document = a_document_of(3);
        document.security = Some(one_password_security());
        built.viewer.state.borrow_mut().session = Some(model_session(document));

        let refusal = built
            .viewer
            .full_rewrite_refusal()
            .expect("a one-password document cannot be rewritten");
        begin_extract(&built.window, &built.viewer);

        assert_eq!(built.viewer.status.text().as_str(), refusal);

        built.window.close();
    }

    /// An open document with no pages has nothing to pull out, and the
    /// refusal has to distinguish that from "no document at all".
    #[gtk::test]
    fn gtk_ui_a_document_with_no_pages_says_so_rather_than_asking_for_a_destination() {
        let built = built_ui();
        built.viewer.state.borrow_mut().session = Some(model_session(Document::blank()));

        begin_extract(&built.window, &built.viewer);

        assert_eq!(
            built.viewer.status.text().as_str(),
            "This document has no pages to extract."
        );

        built.window.close();
    }
}
