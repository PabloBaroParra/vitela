//! Split: cutting the open document into several new PDFs.
//!
//! The Organize screen's "Split" button lands here. It is the sixth write
//! chain and the closest relative of [`extract`](super::extract): both build
//! a second document out of the open one's pages through
//! [`prune`](super::prune), both leave the session completely alone, and both
//! report by naming what landed where. They differ in exactly one thing —
//! **how many files come out** — and that one difference is what makes this a
//! module of its own rather than a flag on that one.
//!
//! ## Why a folder and not a file name
//!
//! The count of files is the count of cuts plus one, so the only question
//! left to ask is where they go; the names are
//! [`pdf_save::split_part_file_name`]'s. That is the same trade
//! [`export`](crate::app::export) makes, and for the same reason: offering a
//! Save-style dialog for a four-way split would be asking a question whose
//! answer cannot be honoured.
//!
//! The consequence is that the shared [`chooser`](super::chooser) is not on
//! this path — it names one destination and guards one overwrite. What
//! replaces it is [`destination`], which asks the same question once
//! for the whole set rather than once per file.
//!
//! ## The permission gates
//!
//! The pair [`extract`](super::extract) settles, for the reasons it sets out
//! at length there: [`Viewer::text_extraction_refusal`] (`/P` bit 5 — lifting
//! content out into another file) and [`Viewer::full_rewrite_refusal`] (a new
//! file with a different page set can only come from the full-rewrite writer,
//! and a document opened with one of its two passwords cannot be re-encrypted
//! at all). Deliberately **not** `page_assembly_refusal`: bit 11 governs
//! assembling *this* document, and a split changes nothing about it.
//!
//! ## How this splits
//!
//! This file owns the chain — the gates and the request it hands on.
//! [`options`] owns the rules the dialog enforces, as plain functions with no
//! widget in sight. [`dialog`] owns the widgets that ask where to cut.
//! [`destination`] owns the folder chooser and the overwrite guard.
//! [`worker`] owns everything after the folder is settled.

mod destination;
mod dialog;
mod options;
mod worker;

use gtk::ApplicationWindow;
use pdf_document::Document;

use super::SPLIT;
use crate::app::state::{ImportedSource, SaveBacking, Viewer};

/// The snapshot a split carries from the live session onto the worker thread.
///
/// Every field is owned: the worker outlives the borrow it was built from,
/// and `document` is a **clone** of the session's model precisely so that
/// pruning it — once per part — cannot touch what the user is still editing,
/// including their undo history, which the pruning writes to.
struct SplitRequest {
    document: Document,
    backing: SaveBacking,
    sources: Vec<ImportedSource>,
    /// Inclusive, zero-based page ranges, one per file, in document order.
    /// See [`options::parts`] for the invariant they hold.
    parts: Vec<(u32, u32)>,
    /// What the written files are named after — the open document's own name
    /// with its extension dropped.
    stem: String,
    /// Generation of the session this split started under.
    /// [`worker::spawn_split`] compares it before touching the status line, so
    /// one that finishes after the document was replaced says nothing.
    generation: u64,
    /// Whether the document this was split from carries a signature — the one
    /// thing the completion message has to add. See [`options::split_summary`]
    /// for why it is a sentence afterwards rather than a modal before.
    signed: bool,
}

/// Opens the Split dialog, or says why it cannot.
pub(crate) fn begin_split(window: &ApplicationWindow, viewer: &Viewer) {
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
        viewer.status.set_text("Open a PDF before splitting it.");
        return;
    };
    // A split has to leave something on both sides of a cut, so the floor is
    // two pages rather than one. Said before the dialog opens, because a
    // dialog whose every possible answer is refused is not a question — and
    // said in two sentences, because "no pages" and "one page" are different
    // documents and a user looking at either should be told which they have.
    if total_pages == 0 {
        viewer
            .status
            .set_text("This document has no pages to split.");
        return;
    }
    if total_pages < 2 {
        viewer
            .status
            .set_text("A document of one page cannot be split.");
        return;
    }

    dialog::prompt_for_cuts(
        window,
        total_pages,
        {
            let viewer = viewer.clone();
            move || viewer.status.set_text(SPLIT.cancelled)
        },
        {
            let viewer = viewer.clone();
            move |window, cuts| match build_request(&viewer, &cuts, total_pages) {
                Ok(request) => destination::choose_folder(window, &viewer, request),
                Err(message) => viewer.status.set_text(&message),
            }
        },
    );
}

/// Bundles what the worker needs out of the live session, or reports why the
/// session can no longer answer for it.
///
/// Read under one borrow and cloned out of it, the way `extract::extract_to`
/// does: the dialog is asynchronous, so everything this reads may have been
/// replaced since the user pressed Split.
fn build_request(viewer: &Viewer, cuts: &[u32], asked_pages: u32) -> Result<SplitRequest, String> {
    let state = viewer.state.borrow();
    let session = state
        .session
        .as_ref()
        .ok_or("Open a PDF before splitting it.")?;
    let document = session
        .document_model
        .clone()
        .ok_or("This document cannot be split as an editable PDF.")?;
    let backing = session
        .save_backing
        .clone()
        .ok_or("This document has no save backing.")?;
    // The cuts were parsed against the page count the dialog opened on. A
    // delete landing while the dialog was up would leave them naming a
    // boundary that no longer exists, and the parts computed from it would
    // run past the end of the model.
    if document.pages.len() as u32 != asked_pages {
        return Err("The document changed while the cuts were being chosen. Try again.".to_owned());
    }
    Ok(SplitRequest {
        signed: pdf_manip::document_has_signatures(&backing.base),
        parts: options::parts(cuts, asked_pages),
        stem: pdf_save::document_file_stem(&session.base_name).to_owned(),
        document,
        backing,
        sources: session.imported_sources.clone(),
        generation: state.generation,
    })
}

#[cfg(test)]
mod tests {
    use gtk::prelude::*;
    use pdf_document::{Document, Orientation, Page, PageId, PageSize};

    use super::begin_split;
    use crate::app::state::TextAccess;
    use crate::app::test_fixtures::{model_session, one_password_security};
    use crate::app::ui_tests::built_ui;

    fn a_document_of(pages: u32) -> Document {
        Document::with_pages(
            (0..pages)
                .map(|id| Page::blank(PageId(id), PageSize::A4, Orientation::Portrait))
                .collect(),
        )
    }

    /// The gate a cold start hits. The path through `begin_split` must say so
    /// rather than open a dialog offering to split nothing.
    #[gtk::test]
    fn gtk_ui_splitting_without_an_open_document_is_refused() {
        let built = built_ui();

        begin_split(&built.window, &built.viewer);

        assert_eq!(
            built.viewer.status.text().as_str(),
            "Open a PDF before splitting it."
        );

        built.window.close();
    }

    /// `/P` bit 5 withheld: refused with the same sentence search, the
    /// selection clipboard, the image export and Extract use, before a dialog
    /// exists.
    #[gtk::test]
    fn gtk_ui_a_document_that_forbids_extraction_is_not_split() {
        let built = built_ui();
        let mut session = model_session(a_document_of(4));
        session.text_access = TextAccess::Forbidden;
        built.viewer.state.borrow_mut().session = Some(session);

        begin_split(&built.window, &built.viewer);

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
    /// rewrite each part needs could never reproduce its encryption. Asked
    /// before the dialog rather than discovered by the worker.
    #[gtk::test]
    fn gtk_ui_a_document_that_cannot_be_rewritten_is_refused_before_the_dialog() {
        let built = built_ui();
        let mut document = a_document_of(4);
        document.security = Some(one_password_security());
        built.viewer.state.borrow_mut().session = Some(model_session(document));

        let refusal = built
            .viewer
            .full_rewrite_refusal()
            .expect("a one-password document cannot be rewritten");
        begin_split(&built.window, &built.viewer);

        assert_eq!(built.viewer.status.text().as_str(), refusal);

        built.window.close();
    }

    /// A one-page document has no cut that leaves something on both sides, so
    /// every answer the dialog could take would be refused. Said here instead
    /// of opening it.
    #[gtk::test]
    fn gtk_ui_a_single_page_document_says_so_rather_than_asking_where_to_cut() {
        let built = built_ui();
        built.viewer.state.borrow_mut().session = Some(model_session(a_document_of(1)));

        begin_split(&built.window, &built.viewer);

        assert_eq!(
            built.viewer.status.text().as_str(),
            "A document of one page cannot be split."
        );

        built.window.close();
    }

    /// An open document with no pages is also below the floor, and the
    /// refusal has to distinguish it from a document with exactly one.
    #[gtk::test]
    fn gtk_ui_a_document_with_no_pages_says_so_rather_than_calling_itself_one_page() {
        let built = built_ui();
        built.viewer.state.borrow_mut().session = Some(model_session(Document::blank()));

        begin_split(&built.window, &built.viewer);

        assert_eq!(
            built.viewer.status.text().as_str(),
            "This document has no pages to split."
        );

        built.window.close();
    }
}
