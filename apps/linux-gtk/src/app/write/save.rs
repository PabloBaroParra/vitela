//! The ordinary Save: settle a destination, warn if the rewrite would
//! invalidate a signature, then serialize the model, validate the bytes and
//! replace the file.
//!
//! Two doors, one chain. [`save`] writes back to the file the document came
//! from when it has one it can replace ([`in_place`](super::in_place) decides)
//! and otherwise asks; [`save_as`] always asks. Both end in
//! [`save_current_to`], so nothing past the destination — the signature
//! warning, the core's refusals, the validating reopen — knows which door it
//! came through. What a save in place adds of its own is *when* it may
//! start: see [`save_in_place`].
//!
//! The reference chain — [`sign`](super::sign) and [`protect`](super::protect)
//! are both described against it, and the shared steps all three take live in
//! [`super`].

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gtk::{gio, glib, ApplicationWindow};
use pdf_document::Document;

use super::super::document::open_document;
use super::super::state::{
    DocumentSource, ImportedSource, OpenedDocument, SaveBacking, SessionToken, Viewer,
};
use super::chooser::{
    confirm_destination, confirm_signature_loss, pick_destination_from, replace_dialog, Proceed,
};
use super::in_place::{replaceable_in_place, route_for, through_links, SaveRoute};
use super::worker::{atomic_write, spawn_write, validate_written_bytes};
use super::{default_save_breaks_signature, imported_sources, SAVE};
use crate::app::content_edit::editor::commit_open_retype;
use crate::app::i18n::{tr, tr_args};

/// Save: writes the current model back to the file it came from, and asks
/// for a destination only when there is no such file to replace. The live
/// session remains untouched until the worker has completed.
pub(crate) fn save(window: &ApplicationWindow, viewer: &Viewer) {
    save_then(window, viewer, None);
}

/// [`save`], with a continuation that runs only once the bytes are on disk —
/// how the unsaved-changes prompt's Save button gets from "keep this work" to
/// the close or reload it was blocking. A save that is cancelled, refused or
/// fails never reaches it.
pub(crate) fn save_then(
    window: &ApplicationWindow,
    viewer: &Viewer,
    after_save: Option<Rc<dyn Fn()>>,
) {
    if !commit_open_retype(viewer) {
        return;
    }
    let Some(opened) = opened_as(viewer) else {
        ask_destination(window, viewer, None, after_save);
        return;
    };
    match route_for(viewer, &opened.path) {
        SaveRoute::InPlace(file) => {
            let destination = Destination {
                file,
                keeps: Some(opened),
            };
            save_in_place(window, viewer, destination, after_save);
        }
        SaveRoute::Ask => ask_destination(window, viewer, Some(opened), after_save),
    }
}

/// Save As: lets the user choose a destination, then persists a snapshot of
/// the current model there.
pub(crate) fn save_as(window: &ApplicationWindow, viewer: &Viewer) {
    if !commit_open_retype(viewer) {
        return;
    }
    ask_destination(window, viewer, opened_as(viewer), None);
}

/// The path and name the active document goes by, when it came from a file.
#[derive(Clone)]
struct OpenedAs {
    /// A document restored after a crash reopens from in-memory bytes, so
    /// this is the only way its Save still points at the file it came from.
    path: PathBuf,
    name: String,
}

fn opened_as(viewer: &Viewer) -> Option<OpenedAs> {
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    Some(OpenedAs {
        path: session.origin_path.clone()?,
        name: session.base_name.clone(),
    })
}

/// Where a save writes, and what the document is called afterwards.
#[derive(Clone)]
struct Destination {
    /// The file the bytes replace.
    file: PathBuf,
    /// The path and name the saved document keeps, for a save in place.
    ///
    /// `file` is then the *resolved* origin, and for a document opened
    /// through a symbolic link that is not the path the user opened: the tab,
    /// the window title and the next Save must go on naming the link. `None`
    /// for any other destination chosen in the chooser, which becomes both.
    keeps: Option<OpenedAs>,
}

/// The chooser, opened on the file the document came from, then the chain.
fn ask_destination(
    window: &ApplicationWindow,
    viewer: &Viewer,
    opened: Option<OpenedAs>,
    after_save: Option<Rc<dyn Fn()>>,
) {
    let suggested = opened.as_ref().map(|opened| opened.path.clone());
    // The chooser binds its answer to the active tab (`tabs::ChooserScope`):
    // a save writes the document it was opened over, never another.
    pick_destination_from(
        window,
        viewer,
        SAVE,
        suggested.as_deref(),
        Rc::new(move |window, viewer, chosen| {
            confirm_chosen(window, viewer, chosen, opened.clone(), after_save.clone());
        }),
    );
}

/// The overwrite guard for a path picked in the chooser, then the chain.
///
/// A path written as it was given goes through the shared guard
/// (`chooser::confirm_destination`), as for any other write. The document's
/// own link is the exception [`chosen_destination`] describes: the bytes go
/// to the file it points at *now*, which is neither the file the guard
/// would ask about nor necessarily the one that was opened — this route is
/// reached precisely when a save in place declined, a read-only target or a
/// re-pointed link included. So that file gets the checks a save in place
/// makes, and the question names it.
fn confirm_chosen(
    window: &ApplicationWindow,
    viewer: &Viewer,
    chosen: PathBuf,
    opened: Option<OpenedAs>,
    after_save: Option<Rc<dyn Fn()>>,
) {
    let destination = chosen_destination(chosen.clone(), opened);
    let Some(link) = destination.keeps.as_ref().map(|keeps| keeps.path.clone()) else {
        let proceed: Proceed = Rc::new(move |window, viewer, file| {
            let destination = Destination { file, keeps: None };
            save_current_to(window, viewer, destination, after_save.clone());
        });
        confirm_destination(window, viewer, SAVE, chosen, proceed);
        return;
    };
    if let Some(refusal) = through_link_refusal(viewer, &link, &destination.file) {
        viewer.refuse(&refusal);
        return;
    }
    let question = tr_args(
        "write-replace-through-link-title",
        &[
            ("target", destination.file.display().to_string().into()),
            ("link", file_name(&link).into()),
        ],
    );
    replace_dialog(&question).choose(Some(window), None::<&gio::Cancellable>, {
        let window = window.clone();
        let viewer = viewer.clone();
        move |response| {
            if response != Ok(1) {
                viewer.info(&SAVE.cancelled());
                return;
            }
            // Asked again now: a tab may have opened the file, or its
            // permissions changed, while the question was up.
            if let Some(refusal) = through_link_refusal(&viewer, &link, &destination.file) {
                viewer.refuse(&refusal);
                return;
            }
            save_current_to(&window, &viewer, destination.clone(), after_save.clone());
        }
    });
}

/// Why a save through the document's own `link` must not write `target`,
/// the file it leads to: the refusals a save in place makes, worded to name
/// the real file.
fn through_link_refusal(viewer: &Viewer, link: &Path, target: &Path) -> Option<String> {
    let key = through_link_refusal_key(
        replaceable_in_place(target),
        crate::app::tabs::path_held_by_other_tab(viewer, target),
    )?;
    Some(tr_args(
        key,
        &[
            ("target", target.display().to_string().into()),
            ("link", file_name(link).into()),
        ],
    ))
}

/// Which refusal, if any, a save through a link makes of a target that is
/// or is not `replaceable` and is or is not `held_by_another_tab`.
fn through_link_refusal_key(replaceable: bool, held_by_another_tab: bool) -> Option<&'static str> {
    if held_by_another_tab {
        Some("write-link-target-open-elsewhere")
    } else if !replaceable {
        Some("write-link-target-not-replaceable")
    } else {
        None
    }
}

/// The last component of `path`, as the user sees it in a folder.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Where a save to `chosen` — a path confirmed in the chooser — writes.
///
/// Any path is written as it was given, a link included: the user picked
/// that name, and what replaces it is a file (`worker::atomic_write`). The
/// one exception is the document's **own** path, when the document was
/// opened through a link. Confirming it is saving the document where it is —
/// what Save does when it has no reason to ask — so the bytes go to the file
/// the link points at and the link stays one, exactly as a save in place
/// leaves it. Replacing the link instead would leave the real file holding
/// the old bytes behind a document that reads as saved.
fn chosen_destination(chosen: PathBuf, opened: Option<OpenedAs>) -> Destination {
    let own = opened.filter(|opened| opened.path == chosen);
    match own.map(|opened| (through_links(&opened.path), opened)) {
        Some((Ok(file), opened)) if file != chosen => Destination {
            file,
            keeps: Some(opened),
        },
        _ => Destination {
            file: chosen,
            keeps: None,
        },
    }
}

/// How often a save held back by a preview refresh looks again.
const SETTLE_TICK: Duration = Duration::from_millis(20);

/// The chain with the destination already known: no chooser and no overwrite
/// question, and otherwise everything a chosen destination goes through.
///
/// **One at a time.** Refused while a job is running on this document: a
/// second Ctrl+S during a save would otherwise start a second write of the
/// same file — and collide with the first on its temporary file — where the
/// chooser used to leave the first one time to finish. Refused rather than
/// folded into the save already under way: that one may carry a continuation
/// (the close it was asked from) and the second another, and one write
/// answering for both would have to promise each of them the other's edits.
///
/// **After the preview.** A content edit refreshes the canvas through a
/// save-and-reopen of its own (`preview::refresh_preview`), which moves the
/// session on a generation when it lands. A disk save started under it would
/// finish against a session that is no longer the one it was written from:
/// the bytes would be on disk and the document still marked unsaved. The
/// chooser used to hide this by taking longer than any refresh; a save with
/// no question to ask has to wait on purpose — Ctrl+S with a retype box open
/// commits the box, which starts exactly such a refresh.
///
/// The wait is itself a job (`tabs::JobGuard`), which is what refuses a
/// second save for as long as it lasts. A refresh is not one, so anything
/// else may have started meanwhile — a signature, an export — and the job
/// state is read again, with the wait's own hold let go, at the moment the
/// save would start.
fn save_in_place(
    window: &ApplicationWindow,
    viewer: &Viewer,
    destination: Destination,
    after_save: Option<Rc<dyn Fn()>>,
) {
    if viewer.jobs_in_flight.get() > 0 {
        viewer.refuse(&tr("save-wait-for-job"));
        return;
    }
    if !preview_in_flight(viewer) {
        save_current_to(window, viewer, destination, after_save);
        return;
    }
    // The tabs hold still while a refresh runs (`tabs::busy_refusal`); the
    // tab is checked once more for the instant between the refresh landing
    // and this noticing.
    let bound = crate::app::tabs::bind_active(viewer);
    let waiting = crate::app::tabs::JobGuard::hold(viewer);
    let mut pending = Some((waiting, window.clone(), destination, after_save));
    let viewer = viewer.clone();
    glib::timeout_add_local(SETTLE_TICK, move || {
        // A document closed under the wait has nothing left to save.
        if viewer.state.borrow().session.is_none() {
            return glib::ControlFlow::Break;
        }
        if preview_in_flight(&viewer) {
            return glib::ControlFlow::Continue;
        }
        let Some((waiting, window, destination, after_save)) = pending.take() else {
            return glib::ControlFlow::Break;
        };
        // Let go first: what is counted below is everyone else's work.
        drop(waiting);
        if !bound.ensure_active_or_report(&viewer) {
            return glib::ControlFlow::Break;
        }
        if viewer.jobs_in_flight.get() > 0 {
            viewer.refuse(&tr("save-wait-for-job"));
            return glib::ControlFlow::Break;
        }
        save_current_to(&window, &viewer, destination, after_save);
        glib::ControlFlow::Break
    });
}

fn preview_in_flight(viewer: &Viewer) -> bool {
    viewer.state.borrow().preview_refresh_in_flight
}

fn save_current_to(
    window: &ApplicationWindow,
    viewer: &Viewer,
    destination: Destination,
    after_save: Option<Rc<dyn Fn()>>,
) {
    let (token, snapshot, base_breaks_signatures) = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            viewer.refuse(&tr("save-no-document"));
            return;
        };
        let Some(document) = session.document_model.clone() else {
            viewer.refuse(&tr("save-not-editable"));
            return;
        };
        let Some(backing) = session.save_backing.clone() else {
            viewer.refuse(&tr("write-no-save-backing"));
            return;
        };
        (
            SessionToken {
                generation: state.generation,
                edit_revision: session.edit_revision,
            },
            SaveSnapshot {
                document,
                backing,
                sources: session.imported_sources.clone(),
            },
            session.base_breaks_signatures,
        )
    };

    if breaks_signature(&snapshot, base_breaks_signatures) {
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
        snapshot,
        destination,
        after_save,
        pdf_save::SignatureAcknowledgement::Unacknowledged,
    );
}

/// Whether this save would break a signature the file carries, asked before
/// the save rather than after a rejected one: `pdf-save` answers the same
/// question either way, and asking here means the user meets the warning as a
/// question instead of an error message.
fn breaks_signature(snapshot: &SaveSnapshot, base_breaks_signatures: bool) -> bool {
    default_save_breaks_signature(
        &snapshot.document,
        &snapshot.backing,
        &snapshot.sources,
        base_breaks_signatures,
    )
}

/// Runs the save on a worker thread and folds the result back into the
/// session. Shared by the ordinary path and the one that had to ask about a
/// signature first, so both reopen and report identically.
fn spawn_save(
    viewer: &Viewer,
    token: SessionToken,
    snapshot: SaveSnapshot,
    destination: Destination,
    after_save: Option<Rc<dyn Fn()>>,
    signatures: pdf_save::SignatureAcknowledgement,
) {
    spawn_write(
        viewer,
        SAVE,
        token,
        &destination.file.clone(),
        after_save,
        move || {
            save_snapshot_and_reopen(
                &snapshot.document,
                &snapshot.backing,
                &snapshot.sources,
                destination,
                signatures,
            )
        },
    );
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
    destination: Destination,
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
    atomic_write(&destination.file, &bytes)?;
    // Reopened from the file just written, not through whatever path the
    // user opened: a link re-pointed meanwhile must not put another file's
    // bytes on screen under this document's name.
    let mut reopened = open_document(
        &DocumentSource::File(destination.file),
        backing.password.as_deref(),
    )
    .map_err(|error| error.to_string())?;
    if let Some(keeps) = destination.keeps {
        reopened.origin_path = Some(keeps.path);
        reopened.name = keeps.name;
    }
    Ok(reopened)
}

#[cfg(test)]
mod tests;
