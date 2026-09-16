//! Compress: writing a smaller copy of the open document (T-199).
//!
//! Home's "Compress" tile lands here. It is a write chain — it produces PDF
//! bytes and puts them on disk through the same [`chooser`](super::chooser)
//! and the same [`atomic_write`](super::worker::atomic_write) the others use —
//! and it shares [`extract`](super::extract)'s defining difference: **nothing
//! is installed afterwards**. Save, Sign and Protect all mean "this file is
//! now the document you are editing". A compression means "there is also a
//! smaller copy of it over there"; the open session is untouched, and the
//! user is left looking at exactly the pages they were.
//!
//! ## Why the destination is asked for *last*
//!
//! Every other chain here asks where to write before it writes anything. This
//! one runs the compression first and only then opens the chooser, and the
//! reason is that a compression is the one operation whose worth cannot be
//! known until it has run. `pdf-compress` guarantees the result is never
//! larger, not that it is ever smaller: a file that is already packed comes
//! back as [`Outcome::NoGain`](pdf_compress::Outcome::NoGain) with the save's
//! own bytes. Asking for a name first would mean asking the user to file a
//! copy of their document before anyone — this shell included — knew whether
//! there was a copy worth filing. Run first, report the two sizes, and ask for
//! a destination only when there is a smaller file to put in it.
//!
//! That ordering is also the whole of [`super::COMPRESS`]'s `done` slot being
//! unused: what the user is told at the end names a path and two sizes, so it
//! is [`options::written_summary`] rather than a constant.
//!
//! ## The gates
//!
//! Two, and both come from `pdf-save` rather than from a policy invented here.
//!
//! [`pdf_save::compression_blocker`] — will these bytes come out encrypted? A
//! protected document cannot be repacked at all: `lopdf` writes every object
//! loose for an encrypted save because the objects are already encrypted by
//! the time the writer sees them. There is no "compress anyway" for this one,
//! so it is a refusal shown before the dialog rather than a question. The
//! yes-path `pdf-save` documents is a consented `SaveIntent::StripProtection`,
//! and no gesture in this shell asks for one.
//!
//! [`pdf_save::compressed_save_will_invalidate_signatures`] — and note that
//! this is deliberately **not** [`pdf_save::will_invalidate_signatures`],
//! which the ordinary Save asks. A compressed save is a full rewrite even when
//! nothing was edited, so a signed document with no pending edits answers
//! `false` to the question Save asks and `true` to this one. Asking Save's
//! question here would warn about a different save than the one about to run.
//!
//! Deliberately **not** `page_assembly_refusal` or `content_edit_refusal`:
//! compressing changes no page and edits no content, and asking a `/P` bit
//! about it would invent a restriction the document never expressed.
//!
//! ## No `Command`, no undo
//!
//! `docs/batch-compress.md` decision 2, and it holds all the way out to the
//! UI: nothing here touches `Document::pending_edits`, no [`EditLog`] entry is
//! written, and the undo button is exactly as it was. Undoing a compression is
//! deleting the file it wrote.
//!
//! [`EditLog`]: pdf_document::EditLog
//!
//! ## How this splits
//!
//! The cut `export` and `extract` make. This file owns the chain — the gates,
//! the snapshot, the signature question. [`options`] owns the rules and every
//! sentence, as plain functions with no widget in sight. [`dialog`] owns the
//! widgets that ask. [`worker`] owns everything from the moment a preset is
//! chosen: the compression, the validation, the destination and the write.

mod dialog;
mod options;
mod worker;

use std::rc::Rc;

use gtk::ApplicationWindow;
use pdf_compress::CompressPreset;
use pdf_document::Document;

use super::chooser::confirm_signature_loss;
use super::{imported_sources, COMPRESS};
use crate::app::state::{ImportedSource, SaveBacking, Viewer};

/// What a compression carries from the live session onto the worker thread.
///
/// Every field is owned, and `document` is a **clone** of the session's model
/// for the reason `ExtractRequest`'s is: the worker may outlive the borrow it
/// was built from, and nothing it does may reach the model the user is still
/// editing.
#[derive(Clone)]
struct CompressRequest {
    document: Document,
    backing: SaveBacking,
    sources: Vec<ImportedSource>,
    /// Generation of the session this compression started under.
    /// [`worker`] compares it before touching the status line, so one that
    /// finishes after the document was replaced says nothing at all.
    generation: u64,
}

/// Opens the Compress dialog, or says why it cannot.
pub(crate) fn begin_compress(viewer: &Viewer) {
    let Some(window) = viewer.window() else {
        return;
    };
    let Some(request) = snapshot(viewer) else {
        return;
    };

    let sources = imported_sources(&request.sources);
    if let Some(refusal) = pdf_save::compression_blocker(save_input(
        &request,
        &sources,
        pdf_save::SignatureAcknowledgement::Unacknowledged,
    )) {
        viewer.status.set_text(&options::refusal_text(&refusal));
        return;
    }

    // The size of the file on disk, not of the save this is about to make —
    // see `dialog` for why the dialog shows the one number it can stand
    // behind and no predicted saving at all.
    let current_size = request.backing.original_bytes.len() as u64;
    dialog::prompt_for_preset(
        &window,
        current_size,
        {
            let viewer = viewer.clone();
            move || viewer.status.set_text(COMPRESS.cancelled)
        },
        {
            let viewer = viewer.clone();
            move |window, preset| compress_with(window, &viewer, preset)
        },
    );
}

/// Takes a fresh snapshot, asks the signature question against it, and starts
/// the worker.
///
/// Re-read rather than carried from [`begin_compress`], the way
/// `extract::extract_to` re-reads: the dialog is asynchronous, so the session
/// it opened over may have been replaced or edited since.
fn compress_with(window: &ApplicationWindow, viewer: &Viewer, preset: CompressPreset) {
    let Some(request) = snapshot(viewer) else {
        return;
    };

    let sources = imported_sources(&request.sources);
    let breaks_signature = pdf_save::compressed_save_will_invalidate_signatures(save_input(
        &request,
        &sources,
        pdf_save::SignatureAcknowledgement::Unacknowledged,
    ))
    .unwrap_or(false);

    if !breaks_signature {
        worker::spawn_compress(
            window,
            viewer,
            request,
            preset,
            pdf_save::SignatureAcknowledgement::Unacknowledged,
        );
        return;
    }

    confirm_signature_loss(window, viewer, COMPRESS, {
        let viewer = viewer.clone();
        let window = window.clone();
        Rc::new(move || {
            worker::spawn_compress(
                &window,
                &viewer,
                request.clone(),
                preset,
                pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
            );
        })
    });
}

/// Everything the chain needs out of the live session, or `None` after
/// reporting why the session cannot answer for it.
///
/// The three refusals are `save`'s own, in `save`'s own words: a compression
/// is a save that happens to be followed by a repack, so a document that
/// cannot be saved cannot be compressed either, and inventing a second
/// vocabulary for the same three conditions would only give the user two
/// names for one problem.
fn snapshot(viewer: &Viewer) -> Option<CompressRequest> {
    let state = viewer.state.borrow();
    let Some(session) = state.session.as_ref() else {
        viewer.status.set_text("Open a PDF before compressing it.");
        return None;
    };
    let Some(document) = session.document_model.clone() else {
        viewer
            .status
            .set_text("This document cannot be saved as an editable PDF.");
        return None;
    };
    let Some(backing) = session.save_backing.clone() else {
        viewer.status.set_text("This document has no save backing.");
        return None;
    };
    Some(CompressRequest {
        document,
        backing,
        sources: session.imported_sources.clone(),
        generation: state.generation,
    })
}

/// The save this compression is about, as both gates and the worker describe
/// it.
///
/// One function rather than three literals, for the reason `pdf-ffi` grew
/// `DocumentState::save_input` in T-196: the two questions asked before the
/// run and the run itself have to describe the **same** save, or the answer
/// the user was given is not about the file that gets written.
fn save_input<'a>(
    request: &'a CompressRequest,
    sources: &'a [(
        pdf_document::ImportedDocumentId,
        &'a pdf_manip::LopdfDocument,
    )],
    signatures: pdf_save::SignatureAcknowledgement,
) -> pdf_save::SaveInput<'a> {
    pdf_save::SaveInput {
        document: &request.document,
        base: &request.backing.base,
        original_bytes: Some(&request.backing.original_bytes),
        // Never `StripProtection`: dropping a document's password protection
        // is a separately consented operation, and "make this smaller" is not
        // consent to publish it unprotected.
        intent: pdf_save::SaveIntent::Default,
        signatures,
        imported_sources: pdf_save::ImportedSources::new(sources),
    }
}

#[cfg(test)]
mod tests {
    use gtk::prelude::*;
    use pdf_document::{Document, Orientation, Page, PageId, PageSize};

    use super::begin_compress;
    use crate::app::test_fixtures::{model_session, one_password_security};
    use crate::app::ui_tests::built_ui;

    fn a_document_of(pages: u32) -> Document {
        Document::with_pages(
            (0..pages)
                .map(|id| Page::blank(PageId(id), PageSize::A4, Orientation::Portrait))
                .collect(),
        )
    }

    /// The gate a cold start hits. The Home tile is live with no document
    /// open — it routes through the file chooser first — so reaching this
    /// function without a session has to say so rather than open a dialog
    /// offering to compress nothing.
    ///
    /// It also pins the window recovery: `begin_compress` returns silently
    /// when `Viewer::window` finds no `ApplicationWindow` to parent a modal
    /// against, so a status message here is proof that lookup worked.
    #[gtk::test]
    fn gtk_ui_compressing_without_an_open_document_is_refused() {
        let built = built_ui();

        begin_compress(&built.viewer);

        assert_eq!(
            built.viewer.status.text().as_str(),
            "Open a PDF before compressing it."
        );

        built.window.close();
    }

    /// A session with a model but no save backing cannot describe the save
    /// the gates are about, and the refusal has to distinguish that from "no
    /// document at all".
    #[gtk::test]
    fn gtk_ui_a_document_with_no_save_backing_says_so_rather_than_opening_the_dialog() {
        let built = built_ui();
        built.viewer.state.borrow_mut().session = Some(model_session(a_document_of(2)));

        begin_compress(&built.viewer);

        assert_eq!(
            built.viewer.status.text().as_str(),
            "This document has no save backing."
        );

        built.window.close();
    }

    /// The encryption gate, met before the dialog rather than discovered by
    /// the worker: bytes that come out encrypted cannot be repacked by
    /// anyone, so there is no preset worth offering and no "compress anyway"
    /// to offer it behind.
    ///
    /// Reaching it needs a save backing, because `compression_blocker` is a
    /// question about a described save — which is exactly why the missing
    /// backing above is refused first.
    #[gtk::test]
    fn gtk_ui_an_encrypted_document_is_refused_in_the_cores_own_words() {
        let built = built_ui();
        let mut document = a_document_of(2);
        document.security = Some(one_password_security());
        let mut session = model_session(document);
        session.save_backing = Some(crate::app::state::SaveBacking {
            base: pdf_manip::LopdfDocument::from_lopdf(gen_fixtures::build_multi_page_document(
                2, "compress",
            )),
            original_bytes: Vec::new(),
            password: None,
        });
        built.viewer.state.borrow_mut().session = Some(session);

        begin_compress(&built.viewer);

        assert_eq!(
            built.viewer.status.text().as_str(),
            super::options::refusal_text(&pdf_compress::Refusal::EncryptedDocumentNotRewritable)
        );

        built.window.close();
    }
}
