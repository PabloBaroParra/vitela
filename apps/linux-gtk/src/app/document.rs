//! Document lifecycle: the file chooser, the built-in sample, the
//! generation-guarded open flow, the encrypted-document password prompt, and
//! background close.

use std::any::Any;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{
    gio, glib, AlertDialog, ApplicationWindow, Box as GtkBox, Button, ContentFit, FileDialog,
    FileFilter, Label, Orientation as GtkOrientation, Overlay, PasswordEntry, Picture, Window,
};
use pdf_document::{Document, Orientation, PageSize, SecurityContext};
use pdf_manip::ManipError;
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderError};
use pdf_sign::CertificateSourcePort;

use super::layout::set_placeholder_size;
use super::render::update_viewport;
use super::search::update_search_controls;
use super::state::{
    AnnotationAccess, ContentEditAccess, DocumentSession, DocumentSource, FitRequest,
    OpenedDocument, PageSlot, PageState, TextAccess, Viewer,
};

/// The sample document, linked into the binary at compile time from the same
/// `assets/sample/` file the Windows and Android shells package. Baking it in
/// rather than reading it at runtime means "Open sample" works from any
/// working directory and survives an install that only copies the executable.
const SAMPLE_PDF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/sample/vitela-sample.pdf"
));

/// Encrypted samples for exercising the password-prompt flow, sourced from
/// the same `tests/fixtures/encrypted/` corpus `pdf-manip`'s decrypt tests
/// use (see `tests/fixtures/README.md`). User passwords: `user-aes-pass` /
/// `user-rc4-pass`.
const AES128_SAMPLE_PDF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/sample/aes_128_user_and_owner.pdf"
));
const RC4_128_SAMPLE_PDF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/sample/rc4_128_user_and_owner.pdf"
));

/// Which built-in sample "Open sample" should load.
#[derive(Clone, Copy)]
pub(crate) enum SampleKind {
    Plain,
    Aes128,
    Rc4128,
}

pub(crate) fn show_file_chooser(window: &ApplicationWindow, viewer: &Viewer) {
    let filter = FileFilter::new();
    filter.set_name(Some("PDF files"));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");
    filter.add_pattern("*.PDF");

    let chooser = FileDialog::builder()
        .title("Open PDF")
        .accept_label("Open")
        .default_filter(&filter)
        .build();
    chooser.open(Some(window), None::<&gio::Cancellable>, {
        let window = window.clone();
        let viewer = viewer.clone();
        move |result| {
            let Ok(file) = result else {
                return;
            };
            let Some(path) = file.path() else {
                viewer
                    .status
                    .set_text("The selected location is not a local file.");
                return;
            };
            open_initial(&window, &viewer, DocumentSource::File(path));
        }
    });
}

/// Lets the user choose a destination, then persists a snapshot of the current
/// model. The live session remains untouched until the worker has completed.
pub(crate) fn show_save_chooser(window: &ApplicationWindow, viewer: &Viewer) {
    show_save_chooser_then(window, viewer, None);
}

/// The save chooser, with an optional continuation that runs only once the
/// bytes are on disk — how the unsaved-changes prompt's Save button gets from
/// "keep this work" to the open it was blocking.
fn show_save_chooser_then(
    window: &ApplicationWindow,
    viewer: &Viewer,
    after_save: Option<Rc<dyn Fn()>>,
) {
    let filter = FileFilter::new();
    filter.set_name(Some("PDF files"));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");
    filter.add_pattern("*.PDF");

    let chooser = FileDialog::builder()
        .title("Save PDF")
        .accept_label("Save")
        .default_filter(&filter)
        .initial_name("document.pdf")
        .build();
    chooser.save(Some(window), None::<&gio::Cancellable>, {
        let window = window.clone();
        let viewer = viewer.clone();
        move |result| {
            let Ok(file) = result else {
                viewer.status.set_text("Save cancelled.");
                return;
            };
            let Some(path) = file.path() else {
                viewer
                    .status
                    .set_text("The selected location is not a local file.");
                return;
            };
            confirm_save_destination(&window, &viewer, pdf_destination(path), after_save.clone());
        }
    });
}

fn pdf_destination(mut path: PathBuf) -> PathBuf {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        path.set_extension("pdf");
    }
    path
}

/// Guards every overwrite behind the same prompt.
///
/// The native chooser raises its own overwrite warning, but only for the name
/// the user typed — appending `.pdf` can land on a file it never checked. One
/// prompt here covers both cases, so a collision is always a question rather
/// than sometimes a dead end the user has to reopen the chooser to escape.
fn confirm_save_destination(
    window: &ApplicationWindow,
    viewer: &Viewer,
    destination: PathBuf,
    after_save: Option<Rc<dyn Fn()>>,
) {
    match destination.try_exists() {
        Ok(false) => save_current_to(window, viewer, destination, after_save),
        Err(error) => viewer.status.set_text(&format!(
            "Could not check whether {} already exists: {error}",
            destination.display()
        )),
        Ok(true) => {
            let dialog = AlertDialog::builder()
                .message("Replace existing PDF?")
                .buttons(["Cancel", "Replace"])
                .cancel_button(0)
                .default_button(1)
                .modal(true)
                .build();
            dialog.choose(Some(window), None::<&gio::Cancellable>, {
                let viewer = viewer.clone();
                let window = window.clone();
                move |response| {
                    if response == Ok(1) {
                        save_current_to(&window, &viewer, destination.clone(), after_save.clone());
                    } else {
                        viewer.status.set_text("Save cancelled.");
                    }
                }
            });
        }
    }
}

/// Saving a signed document breaks its signature, and there is no version of
/// this operation that does not: a rewrite replaces the bytes the signature
/// covers. `pdf-save` refuses such a save unless the caller states the user
/// was told, which is what this prompt is for — the core makes sure the
/// question gets asked, and the answer stays the user's.
///
/// The wording says the signature is *invalidated*, not removed, because that
/// is what happens: nothing in `pdf-save`'s full rewrite strips `/Sig`,
/// `/FT /Sig` or `/AcroForm /SigFlags`, so the saved file still carries the
/// signature and a reader opening it reports it as **invalid**, not absent.
/// Those are different outcomes for someone about to send the file on — one
/// is an unsigned document, the other looks tampered with — and the text has
/// to say which one they are choosing. Keep this and
/// `MainWindow.AskSignatureLossAsync` in the same words.
fn confirm_signature_loss(
    window: &ApplicationWindow,
    viewer: &Viewer,
    document: Document,
    backing: super::state::SaveBacking,
    destination: PathBuf,
    after_save: Option<Rc<dyn Fn()>>,
    token: super::state::SessionToken,
) {
    let dialog = AlertDialog::builder()
        .message("Saving will break this document's signature")
        .detail(
            "This document is signed. Saving rewrites the file, so the signature \
             will no longer match what it covers.\n\n\
             It is not removed: the saved file still carries the signature, and \
             PDF readers will report it as invalid rather than missing.\n\n\
             To keep a copy that still verifies, cancel and save to a different \
             file.",
        )
        .buttons(["Cancel", "Save anyway"])
        .cancel_button(0)
        .default_button(0)
        .modal(true)
        .build();

    dialog.choose(Some(window), None::<&gio::Cancellable>, {
        let viewer = viewer.clone();
        move |response| {
            if response == Ok(1) {
                spawn_save(
                    &viewer,
                    token,
                    document.clone(),
                    backing.clone(),
                    destination.clone(),
                    after_save.clone(),
                    pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
                );
            } else {
                viewer.status.set_text("Save cancelled.");
            }
        }
    });
}

fn save_current_to(
    window: &ApplicationWindow,
    viewer: &Viewer,
    destination: PathBuf,
    after_save: Option<Rc<dyn Fn()>>,
) {
    let (token, document, backing) = {
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
            super::state::SessionToken {
                generation: state.generation,
                edit_revision: session.edit_revision,
            },
            document,
            backing,
        )
    };

    // Asked before the save rather than after a rejected one: `pdf-save`
    // answers the same question either way, and asking here means the user
    // meets the warning as a question instead of an error message.
    let breaks_signature = pdf_save::will_invalidate_signatures(pdf_save::SaveInput {
        document: &document,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
        signatures: pdf_save::SignatureAcknowledgement::Unacknowledged,
    })
    .unwrap_or(false);

    if breaks_signature {
        confirm_signature_loss(
            window,
            viewer,
            document,
            backing,
            destination,
            after_save,
            token,
        );
        return;
    }

    spawn_save(
        viewer,
        token,
        document,
        backing,
        destination,
        after_save,
        pdf_save::SignatureAcknowledgement::Unacknowledged,
    );
}

/// Runs the save on a worker thread and folds the result back into the
/// session. Shared by the ordinary path and the one that had to ask about a
/// signature first, so both reopen and report identically.
#[allow(clippy::too_many_arguments)]
fn spawn_save(
    viewer: &Viewer,
    token: super::state::SessionToken,
    document: Document,
    backing: super::state::SaveBacking,
    destination: PathBuf,
    after_save: Option<Rc<dyn Fn()>>,
    signatures: pdf_save::SignatureAcknowledgement,
) {
    viewer.status.set_text("Saving PDF...");
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let result = gio::spawn_blocking(move || {
                save_snapshot_and_reopen(&document, &backing, &destination, signatures)
            })
            .await;
            let result = save_worker_result(result);
            match result {
                Ok(reopened) if let Some(generation) = prepare_reopened_session(&viewer, token) => {
                    show_document(&viewer, generation, reopened);
                    viewer.status.set_text("PDF saved and reopened.");
                    // Last: the continuation may replace this document, and
                    // its own status text should be what remains on screen.
                    if let Some(after_save) = after_save {
                        after_save();
                    }
                }
                Ok(reopened) => close_document_in_background(reopened.document),
                Err(error) if session_matches(&viewer, token) => viewer
                    .status
                    .set_text(&format!("Could not save PDF: {error}")),
                Err(_) => {}
            }
        }
    });
}

fn session_matches(viewer: &Viewer, token: super::state::SessionToken) -> bool {
    let state = viewer.state.borrow();
    state
        .session
        .as_ref()
        .is_some_and(|session| token.matches(state.generation, session.edit_revision))
}

fn prepare_reopened_session(viewer: &Viewer, token: super::state::SessionToken) -> Option<u64> {
    let mut state = viewer.state.borrow_mut();
    let edit_revision = state.session.as_ref()?.edit_revision;
    let generation = next_generation_if_current(token, state.generation, edit_revision)?;
    state.generation = generation;
    Some(generation)
}

fn next_generation_if_current(
    token: super::state::SessionToken,
    generation: u64,
    edit_revision: u64,
) -> Option<u64> {
    token
        .matches(generation, edit_revision)
        .then(|| generation.saturating_add(1))
}

fn save_worker_result<T>(
    result: Result<Result<T, String>, Box<dyn Any + Send>>,
) -> Result<T, String> {
    result.map_err(|_| "Save worker stopped unexpectedly.".to_owned())?
}

fn save_snapshot_and_reopen(
    document: &Document,
    backing: &super::state::SaveBacking,
    destination: &Path,
    signatures: pdf_save::SignatureAcknowledgement,
) -> Result<OpenedDocument, String> {
    let bytes = pdf_save::save_document(pdf_save::SaveInput {
        document,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
        signatures,
    })
    .map_err(|error| error.to_string())?;
    // Validate before replacing a destination: persisted bytes must be usable
    // by the same renderer path that will display them.
    PdfiumRenderer::new()
        .open_document_from_bytes(bytes.clone(), backing.password.as_deref())
        .map_err(|error| error.to_string())
        .and_then(|handle| {
            PdfiumRenderer::new()
                .close_document(handle)
                .map(|_| ())
                .map_err(|error| error.to_string())
        })?;
    atomic_write(destination, &bytes)?;
    open_document(
        &DocumentSource::File(destination.to_path_buf()),
        backing.password.as_deref(),
    )
    .map_err(|error| error.to_string())
}

/// The inputs [`begin_sign`] threads through the destination chooser to
/// [`spawn_sign`], bundled so the same handful of values do not repeat
/// field-by-field across every step of the chooser → confirm → background-
/// sign chain — the same reason [`SaveBacking`](super::state::SaveBacking)
/// bundles what a save needs instead of three separate parameters.
#[derive(Clone)]
pub(crate) struct SignRequest {
    pub(crate) token: super::state::SessionToken,
    /// `SaveBacking::original_bytes` at the moment the identity was
    /// confirmed. [`super::sign::begin_sign_from_picker`] has already
    /// refused to build this request when `unsaved_to_disk` is set, so this
    /// always matches what the reopened session will show.
    pub(crate) bytes: Vec<u8>,
    pub(crate) password: Option<String>,
    pub(crate) page_number: u32,
    pub(crate) field_name: String,
    pub(crate) source: Arc<dyn CertificateSourcePort>,
    pub(crate) identity_id: String,
}

/// Batch B23 Fase 4/5: the signing twin of [`show_save_chooser`]/
/// [`save_current_to`]. This shell has no notion of "the current file" to
/// overwrite implicitly — even an ordinary Save always asks where to write
/// (`show_save_chooser`) — so signing asks the same way, then runs
/// [`pdf_sign::sign_document`] on a worker thread and reopens the result
/// exactly like a real disk save does.
pub(crate) fn begin_sign(window: &ApplicationWindow, viewer: &Viewer, request: SignRequest) {
    let filter = FileFilter::new();
    filter.set_name(Some("PDF files"));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");
    filter.add_pattern("*.PDF");

    let chooser = FileDialog::builder()
        .title("Save signed PDF")
        .accept_label("Save")
        .default_filter(&filter)
        .initial_name("document.pdf")
        .build();
    chooser.save(Some(window), None::<&gio::Cancellable>, {
        let window = window.clone();
        let viewer = viewer.clone();
        move |result| {
            let Ok(file) = result else {
                viewer.status.set_text("Signing cancelled.");
                return;
            };
            let Some(path) = file.path() else {
                viewer
                    .status
                    .set_text("The selected location is not a local file.");
                return;
            };
            confirm_sign_destination(&window, &viewer, request.clone(), pdf_destination(path));
        }
    });
}

/// [`confirm_save_destination`]'s signing twin: the same "Replace existing
/// PDF?" guard, ending in [`spawn_sign`] instead of [`spawn_save`].
fn confirm_sign_destination(
    window: &ApplicationWindow,
    viewer: &Viewer,
    request: SignRequest,
    destination: PathBuf,
) {
    match destination.try_exists() {
        Ok(false) => spawn_sign(viewer, request, destination),
        Err(error) => viewer.status.set_text(&format!(
            "Could not check whether {} already exists: {error}",
            destination.display()
        )),
        Ok(true) => {
            let dialog = AlertDialog::builder()
                .message("Replace existing PDF?")
                .buttons(["Cancel", "Replace"])
                .cancel_button(0)
                .default_button(1)
                .modal(true)
                .build();
            dialog.choose(Some(window), None::<&gio::Cancellable>, {
                let viewer = viewer.clone();
                move |response| {
                    if response == Ok(1) {
                        spawn_sign(&viewer, request.clone(), destination.clone());
                    } else {
                        viewer.status.set_text("Signing cancelled.");
                    }
                }
            });
        }
    }
}

/// Runs [`pdf_sign::sign_document`] on a worker thread and folds the result
/// back into the session — the signing twin of [`spawn_save`].
fn spawn_sign(viewer: &Viewer, request: SignRequest, destination: PathBuf) {
    viewer.status.set_text("Signing PDF...");
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let token = request.token;
            let result =
                gio::spawn_blocking(move || sign_snapshot_and_reopen(request, &destination)).await;
            let result = save_worker_result(result);
            match result {
                Ok(reopened) if let Some(generation) = prepare_reopened_session(&viewer, token) => {
                    show_document(&viewer, generation, reopened);
                    viewer.status.set_text("PDF signed and reopened.");
                }
                Ok(reopened) => close_document_in_background(reopened.document),
                Err(error) if session_matches(&viewer, token) => viewer
                    .status
                    .set_text(&format!("Could not sign PDF: {error}")),
                Err(_) => {}
            }
        }
    });
}

/// [`save_snapshot_and_reopen`]'s signing twin: signs `request.bytes` instead
/// of serializing a `Document` model, then validates, writes, and reopens
/// exactly the same way.
fn sign_snapshot_and_reopen(
    request: SignRequest,
    destination: &Path,
) -> Result<OpenedDocument, String> {
    let password = request.password;
    let signed = pdf_sign::sign_document(
        request.bytes,
        password.as_deref(),
        request.page_number,
        request.field_name,
        request.source.as_ref(),
        &request.identity_id,
    )
    .map_err(|error| error.to_string())?;
    // Validate before replacing a destination: persisted bytes must be usable
    // by the same renderer path that will display them.
    PdfiumRenderer::new()
        .open_document_from_bytes(signed.clone(), password.as_deref())
        .map_err(|error| error.to_string())
        .and_then(|handle| {
            PdfiumRenderer::new()
                .close_document(handle)
                .map(|_| ())
                .map_err(|error| error.to_string())
        })?;
    atomic_write(destination, &signed)?;
    open_document(
        &DocumentSource::File(destination.to_path_buf()),
        password.as_deref(),
    )
    .map_err(|error| error.to_string())
}

/// Runs the save→reopen→re-render cycle every content-edit commit needs
/// (batch decision 6, `docs/batch-content-edit.md`) — the no-destination
/// twin of [`spawn_save`]. T-161/T-162 deferred this: their commits left the
/// canvas showing pdfium's stale bitmap behind a "Changes are pending save"
/// status, because for an *annotation* the overlay already painted the
/// truth. A content edit changes what pdfium itself renders, so nothing
/// short of a real reopen shows the actual result — that gap stops being
/// deferrable exactly here.
///
/// Modeled closely on [`save_current_to`]/[`spawn_save`]/
/// [`save_snapshot_and_reopen`], with two deliberate differences:
///
/// - **No destination path, no file dialog.** Nothing is written to disk;
///   the reopened handle is built from an in-memory `save_document` buffer
///   only (see [`refresh_snapshot_and_reopen`]).
/// - **No [`confirm_signature_loss`] prompt.** A content edit that would
///   invalidate an existing signature proceeds silently
///   (`pdf_save::SignatureAcknowledgement::ProceedAndInvalidate`). Nothing
///   here is written anywhere durable, so there is nothing irreversible for
///   the user to consent to yet. This only holds because the edit state
///   carried across the reopen ([`EditState`]) keeps the *original*
///   `SaveBacking`: the real disk save therefore still replays a document
///   that `has_content_edits`, still takes the full-rewrite path, and still
///   reaches `confirm_signature_loss` before writing a byte. Installing the
///   reopened session's own backing instead would fold the invalidated
///   signature into the base with an empty edit log, and
///   `will_invalidate_signatures` would then answer `false` for a file whose
///   signature this very function had already broken.
///
/// **This is a preview refresh, not a document open.** [`show_document`] is
/// built to install a *different* document, so it resets everything a new
/// document should reset — including `document_model` (and with it the whole
/// `EditLog`), `save_backing`, the zoom, and the scroll position. Letting it
/// do that here would silently destroy the undo history the user still owns,
/// re-base future saves on bytes that were never written anywhere, and throw
/// the user back to the top of page 1 at fit-width after every single edit.
/// So both halves are lifted out before the call and put back after it
/// ([`take_edit_state`]/[`restore_edit_state`] and
/// [`take_view_state`]/[`restore_view_state`]); only the pdfium handle and
/// the page widgets it feeds are actually replaced.
///
/// Reuses [`prepare_reopened_session`]/[`session_matches`]/
/// [`close_document_in_background`]/[`save_worker_result`] exactly as
/// `spawn_save` does, so a second content edit landing before this one's
/// background save+reopen completes is coalesced the same way a second disk
/// save would be: the stale result's `SessionToken` no longer matches the
/// session's current `(generation, edit_revision)`, so
/// [`prepare_reopened_session`] refuses to install it and it is discarded
/// via [`close_document_in_background`] instead.
///
/// `message` is the status text shown once the refresh lands (e.g. "Text
/// updated.", "Image moved.", "Edit undone.") — no "pending save" suffix,
/// because once this call has run that is no longer true of the *canvas*.
/// The file on disk is still behind, which is what `unsaved_to_disk` tracks.
pub(crate) fn refresh_after_content_edit(viewer: &Viewer, message: &'static str) {
    let (token, document, backing) = {
        let mut state = viewer.state.borrow_mut();
        // Two independent sites can each ask for a refresh off the same
        // click — `content_edit::editor::commit` (retyping a run) and
        // `content_edit::text::finish_text_drag` (dragging one) never
        // coordinate with each other. Starting a second `show_document`
        // teardown/rebuild while the first is still running would race both
        // on the same `viewer.pages` `GtkBox`; deferring the second one
        // until the first finishes (see the tail of the spawned future
        // below) keeps exactly one rebuild in flight at a time.
        if state.content_refresh_in_flight {
            state.content_refresh_pending = Some(message);
            return;
        }
        let Some(session) = state.session.as_ref() else {
            return;
        };
        let Some(document) = session.document_model.clone() else {
            return;
        };
        let Some(backing) = session.save_backing.clone() else {
            return;
        };
        let token = super::state::SessionToken {
            generation: state.generation,
            edit_revision: session.edit_revision,
        };
        state.content_refresh_in_flight = true;
        (token, document, backing)
    };

    viewer.status.set_text("Refreshing preview...");
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let result =
                gio::spawn_blocking(move || refresh_snapshot_and_reopen(&document, &backing)).await;
            let result = save_worker_result(result);
            match result {
                Ok(reopened) if let Some(generation) = prepare_reopened_session(&viewer, token) => {
                    // Lifted out *before* `show_document` drops the session
                    // it belongs to, and put back after — see this function's
                    // own doc for why a preview refresh must not let a
                    // document-open path reset either half.
                    let preserved_edits = take_edit_state(&viewer);
                    let preserved_view = take_view_state(&viewer);
                    show_document(&viewer, generation, reopened);
                    let still_editing = restore_edit_state(&viewer, preserved_edits);
                    restore_view_state(&viewer, generation, preserved_view);
                    // The reopened session's `PageSlot::content` caches start
                    // out empty again (`show_document` builds fresh
                    // `PageSlot`s) — without re-parsing now, the composite-
                    // font/uneditable-run outline would go blank until the
                    // user clicked a run again, the same gap arming the mode
                    // the first time already avoids. Runs after the restore,
                    // so it re-parses the base the restored model's commands
                    // are actually keyed to.
                    if still_editing {
                        super::content_edit::load_all_page_content(&viewer);
                        super::selection::redraw(&viewer);
                    }
                    viewer.status.set_text(message);
                }
                Ok(reopened) => close_document_in_background(reopened.document),
                // The command that triggered this refresh stays recorded in
                // `pending_edits` either way: undo can still remove it, and
                // if the error is a real problem (not just a stale token) an
                // eventual disk Save will hit the exact same error. Rolling
                // the command back here would silently discard an edit the
                // user already confirmed through validate-before-record —
                // worse than leaving a stale preview up with an explanation.
                //
                // `unsaved_to_disk` needs no correction on this path: every
                // caller sets it when it *records* the command, not when the
                // preview catches up, precisely so a failed refresh still
                // reports the document as dirty.
                Err(error) if session_matches(&viewer, token) => viewer
                    .status
                    .set_text(&format!("Could not refresh preview: {error}")),
                Err(_) => {}
            }

            // Replay a refresh that arrived while this one was running
            // instead of dropping it — its edit is already recorded, only
            // the preview is still stale. Cleared first so the replay does
            // not immediately defer against itself.
            let pending = {
                let mut state = viewer.state.borrow_mut();
                state.content_refresh_in_flight = false;
                state.content_refresh_pending.take()
            };
            if let Some(pending_message) = pending {
                refresh_after_content_edit(&viewer, pending_message);
            }
        }
    });
}

/// The half of a session that describes *what the user has edited*, as
/// opposed to what is currently being rendered.
///
/// Exists only so [`refresh_after_content_edit`] can carry it across
/// [`show_document`], which resets it — correctly, for its usual job of
/// installing a different document, and destructively for a preview refresh
/// of the same one.
///
/// The four fields travel together because they are mutually dependent, not
/// because they happen to be convenient: `document_model` holds the
/// annotations `selected_annotation` names and the id space
/// `next_annotation_id` continues, and its `EditLog` is keyed to exactly the
/// `save_backing` it was recorded against. Restoring any of them without the
/// others produces a session that contradicts itself — a selection pointing
/// at nothing, ids colliding with live annotations, or commands replayed
/// against a base they were never validated against.
struct EditState {
    document_model: Option<Document>,
    save_backing: Option<super::state::SaveBacking>,
    next_annotation_id: u64,
    selected_annotation: Option<pdf_document::AnnotationId>,
}

/// Lifts the edit-side state off the current session, leaving the rest of it
/// to be discarded by the [`show_document`] that follows.
///
/// Always paired with [`restore_edit_state`], which is total: it writes onto
/// whatever session is current when it runs. That matters because
/// `show_document` can bail out early (`is_current`) and leave the *old*
/// session in place — in which case the restore simply hands that session
/// back its own fields, rather than stranding it without a model.
fn take_edit_state(viewer: &Viewer) -> Option<EditState> {
    let mut state = viewer.state.borrow_mut();
    let session = state.session.as_mut()?;
    Some(EditState {
        document_model: session.document_model.take(),
        save_backing: session.save_backing.take(),
        next_annotation_id: session.next_annotation_id,
        selected_annotation: session.selected_annotation,
    })
}

/// Puts [`take_edit_state`]'s result back onto the session [`show_document`]
/// has just installed, and reports whether content-edit mode is still armed.
///
/// Also re-asserts `unsaved_to_disk`: the bytes just shown came from an
/// in-memory save that never touched disk, and `show_document` defaults a
/// freshly shown session to `false` — right for an ordinary open or a real
/// disk-save reopen, both of which do match disk, wrong here.
///
/// `update_annotation_controls` runs again afterwards because
/// `show_document` already ran it against the reopened model's *empty*
/// `EditLog` and left Undo/Redo greyed out; the restored model is the one
/// whose history the buttons must reflect.
fn restore_edit_state(viewer: &Viewer, preserved: Option<EditState>) -> bool {
    let still_editing = {
        let mut state = viewer.state.borrow_mut();
        if let Some(session) = state.session.as_mut() {
            if let Some(preserved) = preserved {
                session.document_model = preserved.document_model;
                session.save_backing = preserved.save_backing;
                session.next_annotation_id = preserved.next_annotation_id;
                session.selected_annotation = preserved.selected_annotation;
            }
            session.unsaved_to_disk = true;
        }
        state.content_edit_mode
    };
    super::annotations::update_annotation_controls(viewer);
    still_editing
}

/// Where the user was looking, as opposed to what they had edited
/// ([`EditState`]) or what is being rendered.
///
/// [`show_document`] resets both halves for the same reason: a *different*
/// document has no business inheriting the previous one's zoom or scroll
/// position. Re-showing the same one does.
#[derive(Clone, Copy)]
struct ViewState {
    zoom: super::layout::Zoom,
    reading: super::layout::ReadingPosition,
}

/// Reads the current zoom and reading position. Pure observation — unlike
/// [`take_edit_state`] there is nothing to move out, because
/// [`show_document`] rebuilds these from scratch rather than carrying them.
fn take_view_state(viewer: &Viewer) -> Option<ViewState> {
    // Read before the borrow: `vadjustment()` touches the widget tree, not
    // `viewer.state`, but keeping the two apart is what lets the borrow below
    // stay as short as it is.
    let offset = viewer.scroll.vadjustment().value();
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    Some(ViewState {
        zoom: session.zoom,
        reading: super::layout::reading_position(&session.page_heights, offset),
    })
}

/// Puts the zoom and reading position back after [`show_document`] has reset
/// them to "freshly opened": fit-width, scrolled to the top.
///
/// Order matters. `set_zoom` recomputes every page's box and with it
/// `page_heights`, so the reading position must be resolved *after* it —
/// against the stacking the user will actually be scrolling through, not the
/// fit-width one `show_document` left behind.
fn restore_view_state(viewer: &Viewer, generation: u64, preserved: Option<ViewState>) {
    let Some(preserved) = preserved else {
        return;
    };
    // A no-op when the zoom already matches `show_document`'s fresh default;
    // a full `refresh_layout` when it does not (see its own guard).
    super::layout::set_zoom(viewer, preserved.zoom);

    let target = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            return;
        };
        super::layout::position_offset(&session.page_heights, preserved.reading)
    };
    if target <= 0.0 {
        return;
    }

    // The borrow above must end before this: `set_value` synchronously emits
    // `value_changed`, whose handler borrows the state again — the same
    // sequencing `search::scroll_to_current_match` documents.
    viewer.scroll.vadjustment().set_value(target);

    // Then again on the next idle, because one attempt cannot be enough here.
    // `set_value` clamps to the adjustment's `upper`, and `upper` only tracks
    // the page widgets `show_document` just rebuilt as of the next
    // size-allocate — so the call above lands when the adjustment still holds
    // the pre-rebuild extent (the common case: same document, same page
    // sizes, so the old extent is also the right one) and is clamped short
    // when it does not. Whether GTK clamps synchronously or on its next
    // configure is not something worth depending on, so this re-asserts
    // rather than testing for it. Setting it eagerly first is what keeps the
    // restore from visibly flickering through the top of page 1.
    glib::idle_add_local_once({
        let viewer = viewer.clone();
        move || {
            // A document opened in the meantime owns the scroll position now;
            // this one's is stale.
            if !is_current(&viewer, generation) {
                return;
            }
            let adjustment = viewer.scroll.vadjustment();
            // Only ever scrolls further down, never back up: if the eager
            // call already landed, this is a no-op, and if something else
            // moved past `target` in between, that is more current than what
            // this closure captured.
            if adjustment.value() < target {
                adjustment.set_value(target);
            }
        }
    });
}

/// The no-destination twin of [`save_snapshot_and_reopen`]: saves to an
/// in-memory buffer and reopens *that*, without ever touching disk. See
/// [`refresh_after_content_edit`]'s own doc for why signatures are
/// acknowledged silently here rather than asked about — and why that stays
/// safe only because the caller keeps the original `SaveBacking`.
fn refresh_snapshot_and_reopen(
    document: &Document,
    backing: &super::state::SaveBacking,
) -> Result<OpenedDocument, String> {
    let bytes = pdf_save::save_document(pdf_save::SaveInput {
        document,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
        signatures: pdf_save::SignatureAcknowledgement::ProceedAndInvalidate,
    })
    .map_err(|error| error.to_string())?;
    open_document(&DocumentSource::Bytes(bytes), backing.password.as_deref())
        .map_err(|error| error.to_string())
}

fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "destination has no parent directory".to_string())?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document.pdf"),
        std::process::id()
    ));
    let mut created_temporary = false;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        created_temporary = true;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, destination).map_err(|error| error.to_string())
    })();
    if result.is_err() && created_temporary {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Opens one of the samples that ship inside the binary — the same files the
/// Windows and Android shells package, so all three show identical content.
pub(crate) fn open_sample(window: &ApplicationWindow, viewer: &Viewer, kind: SampleKind) {
    let bytes = match kind {
        SampleKind::Plain => SAMPLE_PDF,
        SampleKind::Aes128 => AES128_SAMPLE_PDF,
        SampleKind::Rc4128 => RC4_128_SAMPLE_PDF,
    };
    open_initial(window, viewer, DocumentSource::Embedded(bytes));
}

/// Opens a local file dropped over a page. A drop has no direct window
/// parameter, so recover the shell window from the status widget.
pub(crate) fn open_file(viewer: &Viewer, path: PathBuf) {
    let Some(window) = viewer
        .status
        .root()
        .and_then(|root| root.downcast::<ApplicationWindow>().ok())
    else {
        viewer
            .status
            .set_text("The application window is unavailable.");
        return;
    };
    open_initial(&window, viewer, DocumentSource::File(path));
}

/// Creates a conventional one-page A4 document, then opens it through the same
/// bytes-based lifecycle used for every other document source.
pub(crate) fn new_blank_document(window: &ApplicationWindow, viewer: &Viewer) {
    let base = pdf_manip::create_blank_document(PageSize::A4, Orientation::Portrait);
    let Ok(base) = pdf_manip::insert_blank_page(&base, 0, PageSize::A4, Orientation::Portrait)
    else {
        viewer.status.set_text("Could not create a blank PDF.");
        return;
    };
    let mut bytes = Vec::new();
    if base.as_lopdf().clone().save_to(&mut bytes).is_err() {
        viewer.status.set_text("Could not create a blank PDF.");
        return;
    }
    open_initial(window, viewer, DocumentSource::Bytes(bytes));
}

/// Every path that replaces the open document funnels through here, so the
/// prompt guarding unsaved work only has to exist once.
fn open_initial(window: &ApplicationWindow, viewer: &Viewer, source: DocumentSource) {
    confirm_replacing_edits(window, viewer, {
        let window = window.clone();
        let viewer = viewer.clone();
        move || start_open(&window, &viewer, source.clone())
    });
}

/// Save, Discard or Cancel for work the open would replace.
///
/// This unifies three behaviours that used to disagree: a drop and Ctrl+N
/// refused outright, while the file chooser replaced the document and reported
/// the loss afterwards. Refusing was a dead end — the counter it consulted
/// could not return to clean — and reporting after the fact was worse, because
/// by then the work was gone. Asking is the answer both were reaching for.
///
/// Cancel is every exit that is not a deliberate choice: the button, Escape,
/// and closing the window. Work is lost only when someone picks Discard.
fn confirm_replacing_edits(
    window: &ApplicationWindow,
    viewer: &Viewer,
    proceed: impl Fn() + 'static,
) {
    if !has_unsaved_changes(viewer) {
        proceed();
        return;
    }

    confirm_unsaved_edits(
        window,
        viewer,
        "Opening another document will discard them.",
        proceed,
    );
}

/// Stops a close request while unsaved work is being resolved.
///
/// The signal itself must return synchronously, while both the confirmation
/// and save flows are asynchronous. A dirty window therefore stays alive
/// until Discard is explicit or Save has reached disk successfully.
pub(crate) fn confirm_closing_edits(
    window: &ApplicationWindow,
    viewer: &Viewer,
) -> glib::Propagation {
    if !has_unsaved_changes(viewer) {
        return glib::Propagation::Proceed;
    }

    let window_to_close = window.clone();
    confirm_unsaved_edits(
        window,
        viewer,
        "Closing Vitela will discard them.",
        move || window_to_close.destroy(),
    );
    glib::Propagation::Stop
}

fn confirm_unsaved_edits(
    window: &ApplicationWindow,
    viewer: &Viewer,
    detail: &'static str,
    proceed: impl Fn() + 'static,
) {
    let dialog = AlertDialog::builder()
        .message("Unsaved changes")
        .detail(format!(
            "The open document has changes that are not saved. {detail}"
        ))
        .buttons(["Cancel", "Discard", "Save"])
        .cancel_button(0)
        .default_button(2)
        .modal(true)
        .build();

    let proceed: Rc<dyn Fn()> = Rc::new(proceed);
    dialog.choose(Some(window), None::<&gio::Cancellable>, {
        let viewer = viewer.clone();
        let window = window.clone();
        move |response| match unsaved_decision(response.ok()) {
            UnsavedDecision::Discard => proceed(),
            UnsavedDecision::Save => {
                save_then(&window, &viewer, proceed.clone());
            }
            UnsavedDecision::Keep => viewer.status.set_text("Kept the unsaved changes."),
        }
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnsavedDecision {
    Keep,
    Discard,
    Save,
}

fn unsaved_decision(response: Option<i32>) -> UnsavedDecision {
    match response {
        Some(1) => UnsavedDecision::Discard,
        Some(2) => UnsavedDecision::Save,
        _ => UnsavedDecision::Keep,
    }
}

/// Runs the save chooser and, only if the bytes reach disk, continues.
///
/// A cancelled chooser or a failed write must not continue: the reader chose
/// Save precisely to keep the work, and opening anyway would discard exactly
/// what they asked to preserve.
fn save_then(window: &ApplicationWindow, viewer: &Viewer, after_save: Rc<dyn Fn()>) {
    show_save_chooser_then(window, viewer, Some(after_save));
}

fn start_open(window: &ApplicationWindow, viewer: &Viewer, source: DocumentSource) {
    let generation = begin_loading(viewer);
    viewer.status.set_text("Opening PDF...");
    glib::spawn_future_local({
        let window = window.clone();
        let viewer = viewer.clone();
        async move {
            match open_in_background(source.clone(), None).await {
                Ok(document) if is_current(&viewer, generation) => {
                    show_document(&viewer, generation, document);
                }
                Ok(document) => close_document_in_background(document.document),
                Err(RenderError::InvalidPassword) if is_current(&viewer, generation) => {
                    prompt_for_password(&window, &viewer, source, generation);
                }
                Err(error) if is_current(&viewer, generation) => viewer
                    .status
                    .set_text(&format!("Could not open PDF: {error}")),
                Err(_) => {}
            }
        }
    });
}

/// Marks the start of a new open attempt and returns its generation.
///
/// This does NOT touch the currently displayed document: a failed or
/// superseded open must leave the previous document on screen. The old
/// session is replaced only once the new one opens successfully, in
/// [`show_document`]. The bumped generation lets [`is_current`] discard the
/// results of any open this one supersedes.
fn begin_loading(viewer: &Viewer) -> u64 {
    // A new open attempt supersedes any password prompt still waiting on the
    // previous one — tear it down here rather than leaving it stacked
    // underneath a second prompt if this attempt also turns out encrypted
    // (observed when a single click somehow re-fires the menu action: two
    // `prompt_for_password` dialogs stack exactly on top of each other, and
    // dismissing the front one "reveals" the back one a moment later).
    let stale_dialog = {
        let mut state = viewer.state.borrow_mut();
        state.generation += 1;
        state.password_dialog.take()
    };
    if let Some(dialog) = stale_dialog {
        dialog.destroy();
    }
    viewer.state.borrow().generation
}

pub(crate) fn is_current(viewer: &Viewer, generation: u64) -> bool {
    viewer.state.borrow().generation == generation
}

async fn open_in_background(
    source: DocumentSource,
    password: Option<String>,
) -> Result<OpenedDocument, RenderError> {
    gio::spawn_blocking(move || open_document(&source, password.as_deref()))
        .await
        .expect("document-open task panicked")
}

fn open_document(
    source: &DocumentSource,
    password: Option<&str>,
) -> Result<OpenedDocument, RenderError> {
    let renderer = PdfiumRenderer::new();
    let document = match source {
        DocumentSource::File(path) => renderer.open_document(path, password)?,
        // pdfium takes ownership of the buffer for the document's lifetime,
        // so the compiled-in slice has to be copied rather than borrowed.
        DocumentSource::Embedded(bytes) => {
            renderer.open_document_from_bytes(bytes.to_vec(), password)?
        }
        DocumentSource::Bytes(bytes) => {
            renderer.open_document_from_bytes(bytes.clone(), password)?
        }
    };
    let security = read_security_context(source, password);
    let text_access = text_access_from(&security);
    let (document_model, save_backing) = read_editable_model(source, password, &security)
        .map_or((None, None), |(model, backing)| {
            (Some(model), Some(backing))
        });
    let annotation_access = annotation_access_from(&security, document_model.is_some());
    let content_edit_access = content_edit_access_from(&security, document_model.is_some());
    // One batched actor round-trip for every page size, instead of N
    // serialized `page_size` round-trips — first paint no longer waits on
    // a per-page metadata sweep for large documents.
    match renderer.page_sizes(document, Priority::Visible).wait() {
        Ok(page_sizes) => Ok(OpenedDocument {
            document,
            page_sizes,
            text_access,
            annotation_access,
            content_edit_access,
            document_model,
            save_backing,
        }),
        Err(error) => {
            let _ = renderer.close_document(document);
            Err(error)
        }
    }
}

/// Reads the document's permissions, once, for every gate that needs them
/// (spec "Open Password-Protected PDF").
///
/// pdfium renders the document but has no view of the lopdf security model
/// the permissions live in, so this asks `pdf-manip` for the same
/// `SecurityContext` the `pdf-ffi` boundary gates on. Only the probe runs — no
/// decrypting load — so it costs one unauthenticated parse on the open worker
/// thread.
///
/// **This must stay the only source of permissions in this shell.** The
/// obvious-looking alternative, reading the context that `open_document`
/// returns alongside the model, is wrong: a document whose *user password is
/// empty* is decrypted in place by lopdf's unauthenticated load, which drops
/// `/Encrypt` from the trailer, so `open_document` sees an unencrypted file
/// and reports **no** context at all — and no context means "unrestricted" to
/// every gate. "Opens with no prompt, yet still restricts" is the single most
/// common shape of restricted PDF in the wild. `read_security_context`
/// recovers the real permissions from lopdf's decoded encryption state
/// instead; `core/pdf-manip/tests/annotation_permission.rs` pins the
/// difference.
fn read_security_context(
    source: &DocumentSource,
    password: Option<&str>,
) -> Result<Option<SecurityContext>, ManipError> {
    match source {
        DocumentSource::File(path) => pdf_manip::read_security_context(path, password),
        DocumentSource::Embedded(bytes) => {
            pdf_manip::read_security_context_from_bytes(bytes, password)
        }
        DocumentSource::Bytes(bytes) => {
            pdf_manip::read_security_context_from_bytes(bytes, password)
        }
    }
}

/// A document pdfium opens but lopdf cannot read is [`TextAccess::Unreadable`]:
/// it still renders, and only text extraction is withheld.
fn text_access_from(security: &Result<Option<SecurityContext>, ManipError>) -> TextAccess {
    match security {
        Ok(security) if pdf_manip::text_extraction_is_allowed(security.as_ref()) => {
            TextAccess::Allowed
        }
        Ok(_) => TextAccess::Forbidden,
        Err(_) => TextAccess::Unreadable,
    }
}

/// Combines the document's *permission* to be annotated with this shell's
/// *ability* to annotate it.
///
/// The two are reported separately on purpose. A document that withholds the
/// annotate bit is [`AnnotationAccess::Forbidden`] and says so; one that
/// permits it but whose editable model could not be built is
/// [`AnnotationAccess::Unavailable`]. Collapsing the second into the first
/// would have the shell claim a restriction the document never declared.
fn annotation_access_from(
    security: &Result<Option<SecurityContext>, ManipError>,
    has_model: bool,
) -> AnnotationAccess {
    match security {
        // Permission is decided first: a document that says no gets that
        // answer whether or not the model happened to build.
        Ok(security) if !pdf_manip::annotation_editing_is_allowed(security.as_ref()) => {
            AnnotationAccess::Forbidden
        }
        Ok(_) if has_model => AnnotationAccess::Allowed,
        _ => AnnotationAccess::Unavailable,
    }
}

/// The content-edit twin of [`annotation_access_from`] — same shape, gated on
/// `pdf_manip::content_editing_is_allowed` instead of the annotate bit.
fn content_edit_access_from(
    security: &Result<Option<SecurityContext>, ManipError>,
    has_model: bool,
) -> ContentEditAccess {
    match security {
        Ok(security) if !pdf_manip::content_editing_is_allowed(security.as_ref()) => {
            ContentEditAccess::Forbidden
        }
        Ok(_) if has_model => ContentEditAccess::Allowed,
        _ => ContentEditAccess::Unavailable,
    }
}

/// Builds the editable core model that annotation commands are recorded
/// against, or `None` when this document cannot be modelled.
///
/// Unlike the permission probe this performs the full decrypting load, because
/// the model needs the actual objects. The `SecurityContext` stored on the
/// model comes from the probe, not from the loader, for the reason spelled out
/// on [`read_security_context`].
fn read_editable_model(
    source: &DocumentSource,
    password: Option<&str>,
    security: &Result<Option<SecurityContext>, ManipError>,
) -> Option<(Document, super::state::SaveBacking)> {
    let security = security.as_ref().ok()?.clone();
    let (base, original_bytes) = match source {
        DocumentSource::File(path) => (
            pdf_manip::open_document(path, password).ok()?.0,
            fs::read(path).ok()?,
        ),
        DocumentSource::Embedded(bytes) => (
            pdf_manip::open_document_from_bytes(bytes, password).ok()?.0,
            bytes.to_vec(),
        ),
        DocumentSource::Bytes(bytes) => (
            pdf_manip::open_document_from_bytes(bytes, password).ok()?.0,
            bytes.clone(),
        ),
    };
    let model = pdf_save::document_from_lopdf(&base, security).ok()?;
    Some((
        model,
        super::state::SaveBacking {
            base,
            original_bytes,
            password: password.map(str::to_owned),
        },
    ))
}

/// The id a freshly placed form field should get (T-141).
///
/// Cannot always start at 0 the way `next_annotation_id` does: an opened
/// PDF's own AcroForm fields are read into `document.form_fields` at open
/// time with ids assigned sequentially from 0
/// (`pdf_form::read_form_fields`), so starting a new field at 0 as well would
/// collide with whatever that read already claimed the moment a document
/// with existing fields is opened. One past the highest id already in use —
/// `0` for an empty (or field-less) set, same as annotations.
fn next_form_field_id(document_model: Option<&Document>) -> u64 {
    document_model
        .map(|document| document.form_fields.iter().map(|field| field.id.0).max())
        .unwrap_or(None)
        .map_or(0, |max| max + 1)
}

fn show_document(viewer: &Viewer, generation: u64, document: OpenedDocument) {
    if !is_current(viewer, generation) {
        close_document_in_background(document.document);
        return;
    }

    // The new document opened successfully: only now replace the previous
    // one. Cancel its in-flight renders and close it, then clear its page
    // widgets before building the new layout.
    {
        let mut state = viewer.state.borrow_mut();
        if let Some(session) = state.session.take() {
            for active in session.active.values() {
                active.cancellation.cancel();
            }
            for active in session.active_tiles.values() {
                active.cancellation.cancel();
            }
            close_document_in_background(session.document);
        }
    }
    while let Some(child) = viewer.pages.first_child() {
        viewer.pages.remove(&child);
    }
    while let Some(child) = viewer.page_navigation.first_child() {
        viewer.page_navigation.remove(&child);
    }

    let fit = FitRequest::measure(viewer);
    let mut slots = Vec::with_capacity(document.page_sizes.len());
    let mut page_heights = Vec::with_capacity(document.page_sizes.len());
    for (page_index, (width_pt, height_pt)) in document.page_sizes.into_iter().enumerate() {
        let picture = Picture::new();
        picture.set_can_shrink(true);
        picture.set_content_fit(ContentFit::Contain);
        let logical_height = set_placeholder_size(&picture, width_pt, height_pt, fit);
        let overlay = Overlay::new();
        overlay.set_child(Some(&picture));
        // Added after the child, so it sits above the rendered page. The tile
        // pipeline keeps it there with `selection::raise_highlights`.
        let highlights = super::selection::build_highlight_layer(viewer, page_index);
        overlay.add_overlay(&highlights);
        viewer.pages.append(&overlay);
        let page_number = page_index + 1;
        let page_button = Button::with_label(&page_number.to_string());
        page_button.set_tooltip_text(Some(&format!("Go to page {page_number}")));
        page_button.update_property(&[gtk::accessible::Property::Label(&format!(
            "Go to page {page_number}"
        ))]);
        page_button.connect_clicked({
            let viewer = viewer.clone();
            move |_| super::navigate_to_page(&viewer, page_index)
        });
        viewer.page_navigation.append(&page_button);
        let box_ = super::layout::resolve_page_box(
            super::layout::Zoom::FitWidth,
            width_pt,
            height_pt,
            fit.viewport(),
        );
        slots.push(PageSlot {
            overlay,
            picture,
            highlights,
            characters: None,
            characters_requested: false,
            content: None,
            width_pt,
            height_pt,
            state: PageState::Idle,
            target_dpi: box_.base_dpi,
            budget: box_.budget(),
            tiles: HashMap::new(),
            tile_dpi: 0,
            tile_generation: 0,
            tile_failed_dpi: 0,
        });
        page_heights.push(logical_height);
    }

    let page_count = slots.len();
    let next_form_field_id = next_form_field_id(document.document_model.as_ref());
    {
        let mut state = viewer.state.borrow_mut();
        state.session_id += 1;
        state.session = Some(DocumentSession {
            document: document.document,
            text_access: document.text_access,
            annotation_access: document.annotation_access,
            content_edit_access: document.content_edit_access,
            document_model: document.document_model,
            save_backing: document.save_backing,
            // Freshly shown: whatever is on screen right now is exactly what
            // this session's bytes came from, which for an ordinary open or a
            // disk-save reopen means it matches disk. A T-163 preview refresh
            // shows bytes that were never written anywhere, so it restores
            // this to `true` right after — see `restore_edit_state`.
            unsaved_to_disk: false,
            edit_revision: 0,
            next_annotation_id: 0,
            selected_annotation: None,
            next_form_field_id,
            selected_form_field: None,
            form_placement: None,
            form_field_drag: None,
            stamp_surfaces: HashMap::new(),
            placement: None,
            annotation_drag: None,
            content_editor: None,
            selected_image: None,
            image_drag: None,
            text_drag: None,
            physical_width: fit.available_width,
            physical_height: fit.available_height,
            scale_factor: fit.scale_factor,
            pages: slots,
            page_heights,
            last_visible: None,
            search: None,
            next_search_id: 0,
            selection: None,
            active: HashMap::new(),
            next_render_id: 0,
            zoom: super::layout::Zoom::FitWidth,
            zoom_generation: 0,
            active_tiles: HashMap::new(),
        });
    }
    super::metadata::refresh(viewer);
    // A new document invalidates the previous document's matches.
    update_search_controls(viewer);
    super::annotations::update_annotation_controls(viewer);
    super::content_edit::update_controls(viewer);
    super::update_content_edit_controls(viewer);
    super::forms::update_forms_controls(viewer);
    // The mode outlives the document it was armed on, so a session installed
    // while it is on has to be given the same start `set_mode` would have —
    // see `content_edit::rearm_for_session`.
    super::content_edit::rearm_for_session(viewer);
    viewer.print_button.set_sensitive(page_count > 0);
    // A document with no pages leaves the page area empty, so the mark stays
    // up — the same call the WinUI shell makes when it re-shows its empty
    // state for a pageless document.
    viewer.app_mark.set_visible(page_count == 0);
    if page_count == 0 {
        viewer.status.set_text("The PDF contains no pages.");
    } else {
        update_viewport(viewer);
    }
}

/// Whether the open document carries changes that have not been written to
/// disk. Read before the session is replaced — see [`show_document`].
///
/// Reads `session.unsaved_to_disk` rather than
/// `document_model.pending_edits.can_undo()` (T-163). The two now agree in
/// the ordinary case — [`refresh_after_content_edit`] carries the `EditLog`
/// across its reopen rather than resetting it — but the flag is still the
/// right question to ask, because it stays `true` on paths where the log
/// cannot speak for itself: a refresh that *failed* after its command was
/// recorded, and the window between recording a command and the async
/// reopen landing. It errs toward asking: undoing every edit back to zero
/// leaves it `true`, so the user is prompted about a document that now
/// matches disk. Prompting once too often is the safe direction; the
/// alternative discards work without asking.
fn has_unsaved_changes(viewer: &Viewer) -> bool {
    viewer
        .state
        .borrow()
        .session
        .as_ref()
        .is_some_and(|session| session.unsaved_to_disk)
}

pub(crate) fn close_document_in_background(document: DocumentHandle) {
    glib::spawn_future_local(async move {
        let _ = gio::spawn_blocking(move || PdfiumRenderer::new().close_document(document)).await;
    });
}

fn prompt_for_password(
    window: &ApplicationWindow,
    viewer: &Viewer,
    source: DocumentSource,
    generation: u64,
) {
    let content = GtkBox::new(GtkOrientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let dialog = Window::builder()
        .transient_for(window)
        .modal(true)
        .title("Password required")
        .child(&content)
        .build();

    let password_entry = PasswordEntry::builder().show_peek_icon(true).build();
    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    let buttons = GtkBox::new(GtkOrientation::Horizontal, 8);
    let cancel = Button::with_label("Cancel");
    let open = Button::with_label("Open");
    buttons.append(&cancel);
    buttons.append(&open);
    content.append(&password_entry);
    content.append(&error_label);
    content.append(&buttons);
    password_entry.grab_focus();

    // Tracked so a later, superseding open attempt can tear this dialog down
    // instead of leaving it stacked behind a second prompt — see
    // `begin_loading` and `dismiss_password_dialog`.
    viewer.state.borrow_mut().password_dialog = Some(dialog.clone());

    let submit: Rc<dyn Fn()> = Rc::new({
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        let password_entry = password_entry.clone();
        let error_label = error_label.clone();
        let source = source.clone();
        move || {
            viewer.status.set_text("Opening password-protected PDF...");
            dialog.set_sensitive(false);
            glib::spawn_future_local({
                let dialog = dialog.clone();
                let viewer = viewer.clone();
                let password_entry = password_entry.clone();
                let error_label = error_label.clone();
                let source = source.clone();
                async move {
                    let password = password_entry.text().to_string();
                    match open_in_background(source, Some(password)).await {
                        Ok(document) if is_current(&viewer, generation) => {
                            show_document(&viewer, generation, document);
                            // See the comment on the cancel branch above:
                            // `close` would re-emit `response(DeleteEvent)`
                            // and stomp the status `show_document` just set.
                            dismiss_password_dialog(&viewer, &dialog);
                        }
                        Ok(document) => close_document_in_background(document.document),
                        Err(RenderError::InvalidPassword) if is_current(&viewer, generation) => {
                            dialog.set_sensitive(true);
                            viewer.status.set_text("Waiting for the document password.");
                            error_label.set_text("The password is incorrect. Try again.");
                            password_entry.set_text("");
                            password_entry.grab_focus();
                        }
                        Err(error) if is_current(&viewer, generation) => {
                            viewer
                                .status
                                .set_text(&format!("Could not open PDF: {error}"));
                            dismiss_password_dialog(&viewer, &dialog);
                        }
                        Err(_) => {}
                    }
                }
            });
        }
    });
    open.connect_clicked({
        let submit = submit.clone();
        move |_| submit()
    });
    password_entry.connect_activate(move |_| submit());
    cancel.connect_clicked({
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        move |_| {
            viewer.status.set_text("Password entry cancelled.");
            dismiss_password_dialog(&viewer, &dialog);
        }
    });
    dialog.present();
}

/// Clears the tracked password dialog and tears it down — but only if it
/// still points at `dialog`. A later open attempt may have already
/// superseded and destroyed this same dialog via `begin_loading`; comparing
/// identity keeps this from clobbering a newer dialog's slot.
fn dismiss_password_dialog(viewer: &Viewer, dialog: &Window) {
    let mut state = viewer.state.borrow_mut();
    if state.password_dialog.as_ref() == Some(dialog) {
        state.password_dialog = None;
    }
    drop(state);
    dialog.destroy();
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        atomic_write, next_form_field_id, next_generation_if_current, pdf_destination,
        save_worker_result, unsaved_decision, UnsavedDecision,
    };
    use crate::app::state::SessionToken;
    use pdf_document::{
        Color, Document, FieldOrigin, FieldValue, FontFamily, FormField, FormFieldId,
        FormFieldKind, PageId, Rect, TextStyle,
    };

    fn a_form_field(id: u64) -> FormField {
        FormField {
            id: FormFieldId(id),
            page: PageId(0),
            name: format!("Text_{id}"),
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
            style: TextStyle {
                font: FontFamily::Helvetica,
                size_pt: 12.0,
                color: Color { r: 0, g: 0, b: 0 },
            },
            value: FieldValue::Text(String::new()),
            kind: FormFieldKind::Text {
                multiline: false,
                max_len: None,
            },
            origin: FieldOrigin::Existing((id as u32, 0)),
        }
    }

    #[test]
    fn a_document_with_no_model_starts_form_field_ids_at_zero() {
        assert_eq!(next_form_field_id(None), 0);
    }

    #[test]
    fn a_document_with_no_existing_fields_starts_at_zero() {
        assert_eq!(next_form_field_id(Some(&Document::blank())), 0);
    }

    #[test]
    fn a_document_with_existing_fields_continues_past_the_highest_id() {
        let mut document = Document::blank();
        document.form_fields.insert(a_form_field(0));
        document.form_fields.insert(a_form_field(1));
        document.form_fields.insert(a_form_field(2));

        assert_eq!(next_form_field_id(Some(&document)), 3);
    }

    #[test]
    fn a_panicked_save_worker_returns_a_typed_save_error() {
        let worker_failure: Box<dyn Any + Send> = Box::new("save worker panic");

        assert_eq!(
            save_worker_result::<()>(Err(worker_failure)),
            Err("Save worker stopped unexpectedly.".to_owned())
        );
    }

    #[test]
    fn a_save_snapshot_error_is_preserved() {
        assert_eq!(
            save_worker_result::<()>(Ok(Err("destination is read-only".to_owned()))),
            Err("destination is read-only".to_owned())
        );
    }

    #[test]
    fn a_matching_save_token_can_install_the_reopened_session_at_the_next_generation() {
        let token = SessionToken {
            generation: 4,
            edit_revision: 2,
        };

        assert_eq!(next_generation_if_current(token, 4, 2), Some(5));
    }

    #[test]
    fn a_stale_save_token_cannot_install_a_reopened_session() {
        let token = SessionToken {
            generation: 4,
            edit_revision: 2,
        };

        assert_eq!(next_generation_if_current(token, 4, 3), None);
    }

    #[test]
    fn cancelling_an_unsaved_changes_prompt_keeps_the_document_open() {
        assert_eq!(unsaved_decision(Some(0)), UnsavedDecision::Keep);
    }

    #[test]
    fn dismissing_an_unsaved_changes_prompt_keeps_the_document_open() {
        assert_eq!(unsaved_decision(None), UnsavedDecision::Keep);
    }

    #[test]
    fn discarding_unsaved_changes_continues_the_blocked_action() {
        assert_eq!(unsaved_decision(Some(1)), UnsavedDecision::Discard);
    }

    #[test]
    fn saving_unsaved_changes_continues_only_through_the_save_path() {
        assert_eq!(unsaved_decision(Some(2)), UnsavedDecision::Save);
    }

    #[test]
    fn a_destination_without_an_extension_gets_a_pdf_suffix() {
        assert_eq!(
            pdf_destination(PathBuf::from("signed-document")),
            PathBuf::from("signed-document.pdf")
        );
    }

    #[test]
    fn a_non_pdf_destination_extension_is_replaced() {
        assert_eq!(
            pdf_destination(PathBuf::from("preview.png")),
            PathBuf::from("preview.pdf")
        );
    }

    #[test]
    fn an_existing_pdf_suffix_is_preserved() {
        assert_eq!(
            pdf_destination(PathBuf::from("signed.PDF")),
            PathBuf::from("signed.PDF")
        );
    }

    #[test]
    fn a_failed_atomic_write_does_not_delete_a_preexisting_temporary_file() {
        let directory = std::env::temp_dir().join(format!(
            "linux-gtk-atomic-write-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&directory).expect("create isolated temporary directory");
        let destination = directory.join("document.pdf");
        let temporary = directory.join(format!(".document.pdf.{}.tmp", std::process::id()));
        fs::write(&temporary, b"another save owns this file").expect("seed colliding temporary");

        let result = atomic_write(&destination, b"new PDF bytes");

        assert!(result.is_err());
        assert_eq!(
            fs::read(&temporary).expect("preexisting temporary remains"),
            b"another save owns this file"
        );
        fs::remove_dir_all(&directory).expect("remove isolated temporary directory");
    }

    #[test]
    fn an_atomic_write_persists_the_complete_destination_bytes() {
        let directory = std::env::temp_dir().join(format!(
            "linux-gtk-atomic-write-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&directory).expect("create isolated temporary directory");
        let destination = directory.join("document.pdf");

        atomic_write(&destination, b"complete PDF bytes").expect("persist destination");

        assert_eq!(
            fs::read(&destination).expect("read persisted destination"),
            b"complete PDF bytes"
        );
        fs::remove_dir_all(&directory).expect("remove isolated temporary directory");
    }
}
