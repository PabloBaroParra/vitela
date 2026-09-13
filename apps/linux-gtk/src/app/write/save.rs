//! The ordinary Save: name a destination, warn if the rewrite would
//! invalidate a signature, then serialize the model, validate the bytes and
//! replace the file.
//!
//! The reference chain — [`sign`](super::sign) and [`protect`](super::protect)
//! are both described against it, and the shared steps all three take live in
//! [`super`].

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::ApplicationWindow;
use pdf_document::Document;

use super::super::document::open_document;
use super::super::state::{
    DocumentSource, ImportedSource, OpenedDocument, SaveBacking, SessionToken, Viewer,
};
use super::chooser::{choose_destination, confirm_signature_loss};
use super::worker::{atomic_write, spawn_write, validate_written_bytes};
use super::{imported_sources, SAVE};

/// Lets the user choose a destination, then persists a snapshot of the current
/// model. The live session remains untouched until the worker has completed.
pub(crate) fn show_save_chooser(window: &ApplicationWindow, viewer: &Viewer) {
    show_save_chooser_then(window, viewer, None);
}

/// The save chooser, with an optional continuation that runs only once the
/// bytes are on disk — how the unsaved-changes prompt's Save button gets from
/// "keep this work" to the open it was blocking.
pub(crate) fn show_save_chooser_then(
    window: &ApplicationWindow,
    viewer: &Viewer,
    after_save: Option<Rc<dyn Fn()>>,
) {
    choose_destination(
        window,
        viewer,
        SAVE,
        Rc::new(move |window, viewer, destination| {
            save_current_to(window, viewer, destination, after_save.clone());
        }),
    );
}

fn save_current_to(
    window: &ApplicationWindow,
    viewer: &Viewer,
    destination: PathBuf,
    after_save: Option<Rc<dyn Fn()>>,
) {
    let (token, document, backing, sources) = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            viewer.status.set_text("Open a PDF before saving.");
            return;
        };
        let Some(document) = session.document_model.clone() else {
            viewer
                .status
                .set_text("This document cannot be saved as an editable PDF.");
            return;
        };
        let Some(backing) = session.save_backing.clone() else {
            viewer.status.set_text("This document has no save backing.");
            return;
        };
        (
            SessionToken {
                generation: state.generation,
                edit_revision: session.edit_revision,
            },
            document,
            backing,
            session.imported_sources.clone(),
        )
    };

    // Asked before the save rather than after a rejected one: `pdf-save`
    // answers the same question either way, and asking here means the user
    // meets the warning as a question instead of an error message.
    let source_refs = imported_sources(&sources);
    let breaks_signature = pdf_save::will_invalidate_signatures(pdf_save::SaveInput {
        document: &document,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
        signatures: pdf_save::SignatureAcknowledgement::Unacknowledged,
        imported_sources: pdf_save::ImportedSources::new(&source_refs),
    })
    .unwrap_or(false);

    if breaks_signature {
        let snapshot = SaveSnapshot {
            document,
            backing,
            sources,
        };
        confirm_signature_loss(window, viewer, SAVE, {
            let viewer = viewer.clone();
            Rc::new(move || {
                spawn_save(
                    &viewer,
                    token,
                    snapshot.clone(),
                    destination.clone(),
                    after_save.clone(),
                    pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
                );
            })
        });
        return;
    }

    spawn_save(
        viewer,
        token,
        SaveSnapshot {
            document,
            backing,
            sources,
        },
        destination,
        after_save,
        pdf_save::SignatureAcknowledgement::Unacknowledged,
    );
}

/// Runs the save on a worker thread and folds the result back into the
/// session. Shared by the ordinary path and the one that had to ask about a
/// signature first, so both reopen and report identically.
fn spawn_save(
    viewer: &Viewer,
    token: SessionToken,
    snapshot: SaveSnapshot,
    destination: PathBuf,
    after_save: Option<Rc<dyn Fn()>>,
    signatures: pdf_save::SignatureAcknowledgement,
) {
    spawn_write(viewer, SAVE, token, after_save, move || {
        save_snapshot_and_reopen(
            &snapshot.document,
            &snapshot.backing,
            &snapshot.sources,
            &destination,
            signatures,
        )
    });
}

#[derive(Clone)]
struct SaveSnapshot {
    document: Document,
    backing: SaveBacking,
    sources: Vec<ImportedSource>,
}

fn save_snapshot_and_reopen(
    document: &Document,
    backing: &SaveBacking,
    sources: &[ImportedSource],
    destination: &Path,
    signatures: pdf_save::SignatureAcknowledgement,
) -> Result<OpenedDocument, String> {
    let source_refs = imported_sources(sources);
    let bytes = pdf_save::save_document(pdf_save::SaveInput {
        document,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
        signatures,
        imported_sources: pdf_save::ImportedSources::new(&source_refs),
    })
    .map_err(|error| error.to_string())?;
    // Validate before replacing a destination: persisted bytes must be usable
    // by the same renderer path that will display them, and must hold exactly
    // the pages the model names — see `reopened_matches_model` for what a
    // disagreement would do to every page index once the file is reopened.
    validate_written_bytes(&bytes, backing.password.as_deref(), document)?;
    atomic_write(destination, &bytes)?;
    open_document(
        &DocumentSource::File(destination.to_path_buf()),
        backing.password.as_deref(),
    )
    .map_err(|error| error.to_string())
}
