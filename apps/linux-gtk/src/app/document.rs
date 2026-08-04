//! Document lifecycle: the file chooser, the built-in sample, the
//! generation-guarded open flow, the encrypted-document password prompt, and
//! background close.

use std::any::Any;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::{
    gio, glib, ApplicationWindow, Dialog, FileChooserAction, FileChooserNative, FileFilter, Label,
    Overlay, PasswordEntry, Picture, ResponseType,
};
use pdf_document::{Document, Orientation, PageSize, SecurityContext};
use pdf_manip::ManipError;
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderError};

use super::layout::set_placeholder_size;
use super::render::update_viewport;
use super::search::update_search_controls;
use super::state::{
    AnnotationAccess, DocumentSession, DocumentSource, FitRequest, OpenedDocument, PageSlot,
    PageState, TextAccess, Viewer,
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

pub(crate) fn show_file_chooser(
    window: &ApplicationWindow,
    viewer: &Viewer,
    active_chooser: &Rc<RefCell<Option<FileChooserNative>>>,
) {
    let filter = FileFilter::new();
    filter.set_name(Some("PDF files"));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");
    filter.add_pattern("*.PDF");

    let chooser = FileChooserNative::new(
        Some("Open PDF"),
        Some(window),
        FileChooserAction::Open,
        Some("Open"),
        Some("Cancel"),
    );
    chooser.add_filter(&filter);
    chooser.connect_response({
        let window = window.clone();
        let viewer = viewer.clone();
        let active_chooser = active_chooser.clone();
        move |chooser, response| {
            active_chooser.replace(None);
            if response != ResponseType::Accept {
                return;
            }

            let Some(path) = chooser.file().and_then(|file| file.path()) else {
                viewer
                    .status
                    .set_text("The selected location is not a local file.");
                return;
            };
            open_initial(&window, &viewer, DocumentSource::File(path));
        }
    });
    chooser.show();
    active_chooser.replace(Some(chooser));
}

/// Lets the user choose a destination, then persists a snapshot of the current
/// model. The live session remains untouched until the worker has completed.
pub(crate) fn show_save_chooser(
    window: &ApplicationWindow,
    viewer: &Viewer,
    active_chooser: &Rc<RefCell<Option<FileChooserNative>>>,
) {
    show_save_chooser_then(window, viewer, active_chooser, None);
}

/// The save chooser, with an optional continuation that runs only once the
/// bytes are on disk — how the unsaved-changes prompt's Save button gets from
/// "keep this work" to the open it was blocking.
fn show_save_chooser_then(
    window: &ApplicationWindow,
    viewer: &Viewer,
    active_chooser: &Rc<RefCell<Option<FileChooserNative>>>,
    after_save: Option<Rc<dyn Fn()>>,
) {
    let filter = FileFilter::new();
    filter.set_name(Some("PDF files"));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");
    filter.add_pattern("*.PDF");

    let chooser = FileChooserNative::new(
        Some("Save PDF"),
        Some(window),
        FileChooserAction::Save,
        Some("Save"),
        Some("Cancel"),
    );
    chooser.add_filter(&filter);
    chooser.set_current_name("document.pdf");
    chooser.connect_response({
        let window = window.clone();
        let viewer = viewer.clone();
        let active_chooser = active_chooser.clone();
        move |chooser, response| {
            active_chooser.replace(None);
            if response != ResponseType::Accept {
                viewer.status.set_text("Save cancelled.");
                return;
            }
            let Some(path) = chooser.file().and_then(|file| file.path()) else {
                viewer
                    .status
                    .set_text("The selected location is not a local file.");
                return;
            };
            confirm_save_destination(&window, &viewer, pdf_destination(path), after_save.clone());
        }
    });
    chooser.show();
    active_chooser.replace(Some(chooser));
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
        Ok(false) => save_current_to(viewer, destination, after_save),
        Err(error) => viewer.status.set_text(&format!(
            "Could not check whether {} already exists: {error}",
            destination.display()
        )),
        Ok(true) => {
            let dialog = Dialog::builder()
                .transient_for(window)
                .modal(true)
                .title("Replace existing PDF?")
                .build();
            dialog.add_button("Cancel", ResponseType::Cancel);
            dialog.add_button("Replace", ResponseType::Accept);
            dialog.connect_response({
                let viewer = viewer.clone();
                move |dialog, response| {
                    dialog.destroy();
                    if response == ResponseType::Accept {
                        save_current_to(&viewer, destination.clone(), after_save.clone());
                    } else {
                        viewer.status.set_text("Save cancelled.");
                    }
                }
            });
            dialog.present();
        }
    }
}

fn save_current_to(viewer: &Viewer, destination: PathBuf, after_save: Option<Rc<dyn Fn()>>) {
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
    viewer.status.set_text("Saving PDF...");
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            let result =
                gio::spawn_blocking(move || save_snapshot(&document, &backing, &destination)).await;
            let result = save_worker_result(result);
            match result {
                Ok(()) if mark_saved_session_clean(&viewer, token) => {
                    super::annotations::update_annotation_controls(&viewer);
                    viewer
                        .status
                        .set_text("PDF saved. Rendering will refresh when reopened.");
                    // Last: the continuation may replace this document, and
                    // its own status text should be what remains on screen.
                    if let Some(after_save) = after_save {
                        after_save();
                    }
                }
                Ok(()) => {}
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

fn mark_saved_session_clean(viewer: &Viewer, token: super::state::SessionToken) -> bool {
    let mut state = viewer.state.borrow_mut();
    let generation = state.generation;
    let Some(session) = state.session.as_mut() else {
        return false;
    };
    let Some(document) = session.document_model.as_mut() else {
        return false;
    };
    clear_saved_edits_if_current(document, token, generation, session.edit_revision)
}

fn clear_saved_edits_if_current(
    document: &mut Document,
    token: super::state::SessionToken,
    generation: u64,
    edit_revision: u64,
) -> bool {
    if !token.matches(generation, edit_revision) {
        return false;
    }
    document.pending_edits = Default::default();
    true
}

fn save_worker_result(
    result: Result<Result<(), String>, Box<dyn Any + Send>>,
) -> Result<(), String> {
    result.map_err(|_| "Save worker stopped unexpectedly.".to_owned())?
}

fn save_snapshot(
    document: &Document,
    backing: &super::state::SaveBacking,
    destination: &Path,
) -> Result<(), String> {
    let bytes = pdf_save::save_document(pdf_save::SaveInput {
        document,
        base: &backing.base,
        original_bytes: Some(&backing.original_bytes),
        intent: pdf_save::SaveIntent::Default,
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
    atomic_write(destination, &bytes)
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
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, destination).map_err(|error| error.to_string())
    })();
    if result.is_err() {
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
/// prompt guarding unsaved annotation work only has to exist once.
fn open_initial(window: &ApplicationWindow, viewer: &Viewer, source: DocumentSource) {
    confirm_replacing_edits(window, viewer, {
        let window = window.clone();
        let viewer = viewer.clone();
        move || start_open(&window, &viewer, source.clone())
    });
}

/// Save, Discard or Cancel for annotation work the open would replace.
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
    if !has_pending_annotation_edits(viewer) {
        proceed();
        return;
    }

    let dialog = Dialog::builder()
        .transient_for(window)
        .modal(true)
        .title("Unsaved annotation changes")
        .build();
    dialog.add_button("Cancel", ResponseType::Cancel);
    dialog.add_button("Discard", ResponseType::Reject);
    dialog.add_button("Save", ResponseType::Accept);
    dialog.set_default_response(ResponseType::Accept);

    let message = Label::new(Some(
        "The open document has annotation changes that are not saved. \
         Opening another document will discard them.",
    ));
    message.set_wrap(true);
    message.set_xalign(0.0);
    dialog.content_area().append(&message);

    let proceed: Rc<dyn Fn()> = Rc::new(proceed);
    dialog.connect_response({
        let viewer = viewer.clone();
        let window = window.clone();
        move |dialog, response| {
            // `destroy`, not `close`: closing a `GtkDialog` re-emits `response`
            // with `DeleteEvent` and re-enters this handler — the same reason
            // `prompt_for_password` destroys.
            dialog.destroy();
            match response {
                ResponseType::Reject => proceed(),
                ResponseType::Accept => {
                    save_then(&window, &viewer, proceed.clone());
                }
                _ => viewer
                    .status
                    .set_text("Kept the unsaved annotation changes."),
            }
        }
    });
    dialog.present();
}

/// Runs the save chooser and, only if the bytes reach disk, continues.
///
/// A cancelled chooser or a failed write must not continue: the reader chose
/// Save precisely to keep the work, and opening anyway would discard exactly
/// what they asked to preserve.
fn save_then(window: &ApplicationWindow, viewer: &Viewer, after_save: Rc<dyn Fn()>) {
    // Owned here rather than shared with the toolbar's chooser slot: this one
    // lives exactly as long as the prompt that opened it.
    let holder: Rc<RefCell<Option<FileChooserNative>>> = Rc::new(RefCell::new(None));
    show_save_chooser_then(window, viewer, &holder, Some(after_save));
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
    // One batched actor round-trip for every page size, instead of N
    // serialized `page_size` round-trips — first paint no longer waits on
    // a per-page metadata sweep for large documents.
    match renderer.page_sizes(document, Priority::Visible).wait() {
        Ok(page_sizes) => Ok(OpenedDocument {
            document,
            page_sizes,
            text_access,
            annotation_access,
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

    let fit = FitRequest::measure(viewer);
    let mut slots = Vec::with_capacity(document.page_sizes.len());
    let mut page_heights = Vec::with_capacity(document.page_sizes.len());
    for (page_index, (width_pt, height_pt)) in document.page_sizes.into_iter().enumerate() {
        let picture = Picture::new();
        picture.set_can_shrink(true);
        picture.set_keep_aspect_ratio(true);
        let logical_height = set_placeholder_size(&picture, width_pt, height_pt, fit);
        let overlay = Overlay::new();
        overlay.set_child(Some(&picture));
        // Added after the child, so it sits above the rendered page. The tile
        // pipeline keeps it there with `selection::raise_highlights`.
        let highlights = super::selection::build_highlight_layer(viewer, page_index);
        overlay.add_overlay(&highlights);
        viewer.pages.append(&overlay);
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
    {
        let mut state = viewer.state.borrow_mut();
        state.session_id += 1;
        state.session = Some(DocumentSession {
            document: document.document,
            text_access: document.text_access,
            annotation_access: document.annotation_access,
            document_model: document.document_model,
            save_backing: document.save_backing,
            edit_revision: 0,
            next_annotation_id: 0,
            selected_annotation: None,
            stamp_surfaces: HashMap::new(),
            placement: None,
            annotation_drag: None,
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
    // A new document invalidates the previous document's matches.
    update_search_controls(viewer);
    super::annotations::update_annotation_controls(viewer);
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

/// Whether the open document carries annotation edits that have not been
/// written anywhere. Read before the session is replaced — see
/// [`show_document`].
fn has_pending_annotation_edits(viewer: &Viewer) -> bool {
    viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.document_model.as_ref())
        .is_some_and(|document| document.pending_edits.can_undo())
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
    let dialog = Dialog::builder()
        .transient_for(window)
        .modal(true)
        .title("Password required")
        .build();
    dialog.add_button("Cancel", ResponseType::Cancel);
    dialog.add_button("Open", ResponseType::Accept);
    dialog.set_default_response(ResponseType::Accept);

    let password_entry = PasswordEntry::builder()
        .show_peek_icon(true)
        .activates_default(true)
        .build();
    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    dialog.content_area().append(&password_entry);
    dialog.content_area().append(&error_label);
    password_entry.grab_focus();

    // Tracked so a later, superseding open attempt can tear this dialog down
    // instead of leaving it stacked behind a second prompt — see
    // `begin_loading` and `dismiss_password_dialog`.
    viewer.state.borrow_mut().password_dialog = Some(dialog.clone());

    dialog.connect_response({
        let viewer = viewer.clone();
        move |dialog, response| {
            if response != ResponseType::Accept {
                viewer.status.set_text("Password entry cancelled.");
                // `destroy`, not `close`: closing a `GtkDialog` re-emits
                // `response` with `DeleteEvent`, which would re-enter this
                // same handler right after we just set the status above.
                dismiss_password_dialog(&viewer, dialog);
                return;
            }

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
    dialog.present();
}

/// Clears the tracked password dialog and tears it down — but only if it
/// still points at `dialog`. A later open attempt may have already
/// superseded and destroyed this same dialog via `begin_loading`; comparing
/// identity keeps this from clobbering a newer dialog's slot.
fn dismiss_password_dialog(viewer: &Viewer, dialog: &Dialog) {
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
    use std::path::PathBuf;

    use pdf_document::{Command, Document, Orientation, Page, PageId, PageSize};

    use super::{clear_saved_edits_if_current, pdf_destination, save_worker_result};
    use crate::app::state::SessionToken;

    fn document_with_pending_edit() -> Document {
        let mut document = Document::blank();
        let mut edits = std::mem::take(&mut document.pending_edits);
        edits.apply(
            &mut document,
            Command::InsertPage {
                index: 0,
                page: Page::blank(PageId(0), PageSize::A4, Orientation::Portrait),
            },
        );
        document.pending_edits = edits;
        document
    }

    #[test]
    fn a_panicked_save_worker_returns_a_typed_save_error() {
        let worker_failure: Box<dyn Any + Send> = Box::new("save worker panic");

        assert_eq!(
            save_worker_result(Err(worker_failure)),
            Err("Save worker stopped unexpectedly.".to_owned())
        );
    }

    #[test]
    fn a_save_snapshot_error_is_preserved() {
        assert_eq!(
            save_worker_result(Ok(Err("destination is read-only".to_owned()))),
            Err("destination is read-only".to_owned())
        );
    }

    #[test]
    fn a_matching_save_token_clears_pending_edits() {
        let mut document = document_with_pending_edit();

        let cleared = clear_saved_edits_if_current(
            &mut document,
            SessionToken {
                generation: 4,
                edit_revision: 2,
            },
            4,
            2,
        );

        assert!(cleared && !document.pending_edits.can_undo());
    }

    #[test]
    fn a_stale_save_token_preserves_pending_edits() {
        let mut document = document_with_pending_edit();

        let cleared = clear_saved_edits_if_current(
            &mut document,
            SessionToken {
                generation: 4,
                edit_revision: 2,
            },
            4,
            3,
        );

        assert!(!cleared && document.pending_edits.can_undo());
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
}
