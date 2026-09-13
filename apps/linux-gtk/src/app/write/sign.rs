//! The signing save: the same chooser → replace guard → worker-thread write
//! → reopen as [`save`](super::save), running `pdf_sign::sign_document` over
//! the bytes already on disk instead of serializing the model.
//!
//! The identity picker and the certificate prompts that produce a
//! [`SignRequest`] live in `app::sign`; everything from "where should this go"
//! onwards is here.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gtk::ApplicationWindow;
use pdf_render::PdfiumRenderer;
use pdf_sign::CertificateSourcePort;

use super::super::document::open_document;
use super::super::state::{DocumentSource, OpenedDocument, SessionToken, Viewer};
use super::chooser::choose_destination;
use super::worker::{atomic_write, spawn_write};
use super::SIGN;

/// The inputs [`begin_sign`] threads through the destination chooser to
/// [`spawn_sign`], bundled so the same handful of values do not repeat
/// field-by-field across every step of the chooser → confirm → background-
/// sign chain — the same reason [`SaveBacking`](crate::app::state::SaveBacking)
/// bundles what a save needs instead of three separate parameters.
#[derive(Clone)]
pub(crate) struct SignRequest {
    pub(crate) token: SessionToken,
    /// `SaveBacking::original_bytes` at the moment the identity was
    /// confirmed. `sign::begin_sign_from_picker` has already
    /// refused to build this request when `unsaved_to_disk` is set, so this
    /// always matches what the reopened session will show.
    pub(crate) bytes: Vec<u8>,
    pub(crate) password: Option<String>,
    pub(crate) page_number: u32,
    pub(crate) field_name: String,
    pub(crate) source: Arc<dyn CertificateSourcePort>,
    pub(crate) identity_id: String,
}

/// Batch B23 Fase 4/5: the signing twin of
/// [`show_save_chooser`](super::save::show_save_chooser). This shell has no
/// notion of "the current file" to overwrite implicitly — even an ordinary
/// Save always asks where to write — so signing asks the same way, then runs
/// [`pdf_sign::sign_document`] on a worker thread and reopens the result
/// exactly like a real disk save does.
pub(crate) fn begin_sign(window: &ApplicationWindow, viewer: &Viewer, request: SignRequest) {
    choose_destination(
        window,
        viewer,
        SIGN,
        Rc::new(move |_window, viewer, destination| {
            spawn_sign(viewer, request.clone(), destination);
        }),
    );
}

/// Runs [`pdf_sign::sign_document`] on a worker thread and folds the result
/// back into the session — the signing twin of `save::spawn_save`.
fn spawn_sign(viewer: &Viewer, request: SignRequest, destination: PathBuf) {
    let token = request.token;
    spawn_write(viewer, SIGN, token, None, move || {
        sign_snapshot_and_reopen(request, &destination)
    });
}

/// `save::save_snapshot_and_reopen`'s signing twin: signs `request.bytes`
/// instead of serializing a `Document` model, then validates, writes, and
/// reopens exactly the same way.
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
    //
    // Not `worker::validate_written_bytes`: signing has no `Document` model
    // to check a page count against — it never had one, because it signs
    // bytes rather than serializing a model — so opening the file is the
    // whole check here.
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
