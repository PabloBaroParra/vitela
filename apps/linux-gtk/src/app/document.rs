//! Document lifecycle: the file chooser, the built-in sample, the
//! generation-guarded open flow, the encrypted-document password prompt, and
//! background close.
//!
//! Everything that writes a document out lives in [`super::write`] instead —
//! Save, signing, protecting and the content-edit preview refresh. The two
//! halves meet at [`show_document`], which every one of those chains reaches
//! to install the document it reopened, and at the unsaved-changes prompt
//! below, which is the one place lifecycle has to run a save itself.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gio, glib, AlertDialog, ApplicationWindow, Box as GtkBox, Button, ContentFit, FileDialog,
    Label, Orientation as GtkOrientation, Overlay, PasswordEntry, Picture, Window,
};
use pdf_document::{Document, FieldValue, Orientation, PageSize, SecurityContext};
use pdf_manip::ManipError;
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderError};

use super::layout::set_placeholder_size;
use super::render::update_viewport;
use super::search::update_search_controls;
use super::state::{
    AnnotationAccess, ContentEditAccess, DocumentSession, DocumentSource, FitRequest,
    OpenedDocument, PageAssemblyAccess, PageSlot, PageState, TextAccess, Viewer,
};
use super::write::pdf_filter;

/// The sample document, linked into the binary at compile time from the same
/// `assets/sample/` file the Windows and Android shells package. Baking it in
/// rather than reading it at runtime means "Open sample" works from any
/// working directory and survives an install that only copies the executable.
pub(crate) const SAMPLE_PDF: &[u8] = include_bytes!(concat!(
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
    let filter = pdf_filter();
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

/// The page ids of `model`, in its own page order — the order any bytes
/// saved from it are written in, and therefore the page order of the pdfium
/// handle opened from those bytes.
///
/// `None` (a document with no editable model) yields an empty order rather
/// than a guess: without a model there are no page ids to name, and every
/// consumer treats "not in the backend order" as "nothing to draw".
pub(crate) fn backend_page_order(model: Option<&Document>) -> Vec<pdf_document::PageId> {
    model
        .map(|model| model.pages.iter().map(|page| page.id).collect())
        .unwrap_or_default()
}

/// The form-field values `model` carries, keyed by `/T` name — the record of
/// what pdfium is about to draw by itself.
///
/// Read from the model beside the handle for the same reason
/// [`backend_page_order`] is: both came out of the same bytes, so what this
/// model says about a field is what the widget in those bytes says, and a
/// widget in those bytes is a widget pdfium rasterizes. See
/// [`DocumentSession::rendered_field_values`] for why it is keyed by name and
/// why a preview refresh must not restore it from the preserved model.
///
/// [`DocumentSession::rendered_field_values`]: super::state::DocumentSession::rendered_field_values
fn rendered_field_values(model: Option<&Document>) -> HashMap<String, FieldValue> {
    model
        .map(|model| {
            model
                .form_fields
                .iter()
                .map(|field| (field.name.clone(), field.value.clone()))
                .collect()
        })
        .unwrap_or_default()
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
    super::write::show_save_chooser_then(window, viewer, Some(after_save));
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
                    remember_recent(&source);
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

/// Registers a successfully opened file with the desktop's recent-files
/// store, so Home's Recent list — and every other application's — carries it
/// next time.
///
/// Only real files: the compiled-in sample and Ctrl+N's in-memory document
/// have no path to register, and a recent entry the user cannot get back to
/// from a file manager is not recent, it is noise. Called on success only —
/// registering on *attempt* would fill the list with files that turned out
/// unreadable.
fn remember_recent(source: &DocumentSource) {
    if let DocumentSource::File(path) = source {
        super::home::recents::remember(path);
    }
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

pub(crate) fn open_document(
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
    let page_assembly_access = page_assembly_access_from(&security);
    // One batched actor round-trip for every page size, instead of N
    // serialized `page_size` round-trips — first paint no longer waits on
    // a per-page metadata sweep for large documents.
    match renderer.page_geometry(document, Priority::Visible).wait() {
        Ok(page_geometry) => Ok(OpenedDocument {
            document,
            name: source_name(source),
            page_geometry,
            text_access,
            annotation_access,
            content_edit_access,
            page_assembly_access,
            document_model,
            save_backing,
        }),
        Err(error) => {
            let _ = renderer.close_document(document);
            Err(error)
        }
    }
}

/// What to call the document this source opens — see
/// [`DocumentSession::base_name`].
///
/// A `Bytes` source has no name of its own: it is either Ctrl+N's blank
/// document or a preview refresh's in-memory save, and the refresh restores
/// the name it had rather than taking this fallback (`restore_edit_state`).
fn source_name(source: &DocumentSource) -> String {
    match source {
        DocumentSource::File(path) => path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Opened PDF")
            .to_owned(),
        DocumentSource::Embedded(_) => "Sample document".to_owned(),
        DocumentSource::Bytes(_) => "Untitled document".to_owned(),
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

/// The page-assembly twin of [`text_access_from`] — the same three-state
/// shape, because like text extraction this is a pure permission question:
/// the Organize screen reports a missing editable model separately, in its
/// own words.
fn page_assembly_access_from(
    security: &Result<Option<SecurityContext>, ManipError>,
) -> PageAssemblyAccess {
    match security {
        Ok(security) if pdf_manip::document_assembly_is_allowed(security.as_ref()) => {
            PageAssemblyAccess::Allowed
        }
        Ok(_) => PageAssemblyAccess::Forbidden,
        Err(_) => PageAssemblyAccess::Unreadable,
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

/// The overlay one page is drawn in: its render underneath, and room above it
/// for the highlight layer and the tile pictures.
///
/// Centred rather than filled, and that is the whole reason this is a function
/// rather than three lines inline. `viewer.pages` is a vertical box, so a
/// `Fill`-aligned child takes the box's width — which is the *widest* page's.
/// A narrower page's `Picture` then centres its render inside that extra room,
/// while the highlight layer, the tile pictures and every pointer-to-PDF
/// mapping go on measuring from the overlay's own left edge. Everything the
/// shell draws on such a page lands half the width difference to the left of
/// the page it belongs to.
///
/// Turning one page to landscape is the easiest way to end up with pages of
/// different widths, but nothing here is particular to rotation: any document
/// that mixes page sizes had it.
fn page_overlay(picture: &Picture) -> Overlay {
    let overlay = Overlay::new();
    overlay.set_child(Some(picture));
    overlay.set_halign(gtk::Align::Center);
    overlay
}

pub(crate) fn show_document(viewer: &Viewer, generation: u64, document: OpenedDocument) {
    if !is_current(viewer, generation) {
        close_document_in_background(document.document);
        return;
    }

    super::organize::document_changed(viewer);

    // Before the measuring below, not after: a document on screen means the
    // editor page, whichever view the open was started from (Home's drop
    // zone, a recents card, Ctrl+O, a file-manager launch), and `FitRequest::
    // measure` reads an allocation the stack only gives a page it is showing.
    // A first open still measures one frame early — `refresh_layout`, wired
    // to the scroller's page-size change, refits as soon as the real
    // allocation lands.
    super::home::show_editor(viewer);

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
    let mut slots = Vec::with_capacity(document.page_geometry.len());
    let mut page_heights = Vec::with_capacity(document.page_geometry.len());
    for (page_index, geometry) in document.page_geometry.into_iter().enumerate() {
        let (width_pt, height_pt) = (geometry.width_pt, geometry.height_pt);
        let picture = Picture::new();
        picture.set_can_shrink(true);
        picture.set_content_fit(ContentFit::Contain);
        let logical_height = set_placeholder_size(&picture, width_pt, height_pt, fit);
        let overlay = page_overlay(&picture);
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
            rotation: geometry.rotation,
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
    // The handle installed below and the model beside it were read from the
    // same bytes, so pdfium's page order *is* this model's page order. A
    // preview refresh reopens with a model that is thrown away again a moment
    // later, so it re-installs this from the preserved one — see
    // `restore_edit_state`.
    let backend_pages = backend_page_order(document.document_model.as_ref());
    // Same "the model beside the handle describes the handle" reasoning as
    // `backend_pages` — but unlike it, this one is deliberately *not*
    // re-installed by `restore_edit_state`. See its field doc.
    let rendered_field_values = rendered_field_values(document.document_model.as_ref());
    {
        let mut state = viewer.state.borrow_mut();
        state.session_id += 1;
        state.session = Some(DocumentSession {
            document: document.document,
            base_name: document.name,
            text_access: document.text_access,
            annotation_access: document.annotation_access,
            content_edit_access: document.content_edit_access,
            page_assembly_access: document.page_assembly_access,
            document_model: document.document_model,
            backend_pages,
            rendered_field_values,
            save_backing: document.save_backing,
            imported_sources: Vec::new(),
            import_warning_revision: None,
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
    super::sign::update_sign_controls(viewer);
    // The mode outlives the document it was armed on, so a session installed
    // while it is on has to be given the same start `set_mode` would have —
    // see `content_edit::rearm_for_session`.
    super::content_edit::rearm_for_session(viewer);
    viewer.print_button.set_sensitive(page_count > 0);
    // `save_button` was wired to `show_save_chooser` from the start (see
    // `mod.rs`'s `connect_clicked`) but never got the other half of the pair
    // its own comment already describes: nothing ever flipped it back on
    // after `editor_toolbar::build_editor_toolbar`'s initial `set_sensitive
    // (false)`, so the toolbar's Save button stayed permanently greyed out —
    // saving only ever worked through the `Ctrl+S` accelerator. Same gate as
    // `print_button` right above: there is nothing to save or print with no
    // pages on screen.
    viewer.save_button.set_sensitive(page_count > 0);
    // Same gate again: there are no pages to rasterize into images
    // either. The permission question the export also asks is left to
    // `export::begin_export`, so a restricted document still offers the
    // button and explains itself, rather than greying out in silence.
    viewer.export_button.set_sensitive(page_count > 0);
    // A document with no pages leaves the page area empty, so the mark stays
    // up — the same call the WinUI shell makes when it re-shows its empty
    // state for a pageless document.
    viewer.app_mark.set_visible(page_count == 0);
    if page_count == 0 {
        viewer.status.set_text("The PDF contains no pages.");
    } else {
        update_viewport(viewer);
    }
    // Last, so a tool armed on Home before there was a document to apply it
    // to lands on controls whose sensitivity the `update_*` calls above have
    // already settled for *this* document.
    super::home::apply_pending_tool(viewer);
}

/// Whether the open document carries changes that have not been written to
/// disk. Read before the session is replaced — see [`show_document`].
///
/// Reads `session.unsaved_to_disk` rather than
/// `document_model.pending_edits.can_undo()` (T-163). The two now agree in
/// the ordinary case — [`refresh_preview`] carries the `EditLog`
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
                    match open_in_background(source.clone(), Some(password)).await {
                        Ok(document) if is_current(&viewer, generation) => {
                            show_document(&viewer, generation, document);
                            remember_recent(&source);
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
    use super::{
        next_form_field_id, page_overlay, rendered_field_values, unsaved_decision, UnsavedDecision,
    };
    use crate::app::test_fixtures::a_form_field;
    use gtk::prelude::*;
    use gtk::{glib, Box as GtkBox, ContentFit, Orientation, Picture, Window};
    use pdf_document::{Document, FieldValue};

    /// Pumps the main loop until `widget` has been allocated, so a test can
    /// read real widths rather than the zeroes of an unmapped window.
    fn settle(widget: &impl IsA<gtk::Widget>) {
        let context = glib::MainContext::default();
        for _ in 0..2_000 {
            while context.iteration(false) {}
            if widget.as_ref().width() > 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    /// The bug this guards: a document holding one landscape page — a portrait
    /// one turned a quarter — and portrait pages beside it. `viewer.pages` is
    /// then as wide as the landscape page, and a filled overlay would hand
    /// every narrower page that width. The page's own render centres itself in
    /// the surplus while the highlight layer on top of it does not, so every
    /// outline, every tile and every press lands half the difference to the
    /// left of the page it belongs to.
    ///
    /// Asserted on the real allocation rather than on `halign`, because the
    /// property is only the means: what has to hold is that the overlay and the
    /// page inside it are one box.
    #[gtk::test]
    fn gtk_ui_a_narrow_pages_overlay_is_no_wider_than_its_own_page() {
        const WIDE: i32 = 792;
        const NARROW: i32 = 612;

        let pages = GtkBox::new(Orientation::Vertical, 8);
        // As `app::build_window` sets it up.
        pages.set_halign(gtk::Align::Center);

        let page = |width: i32, height: i32| {
            let picture = Picture::new();
            picture.set_can_shrink(true);
            picture.set_content_fit(ContentFit::Contain);
            picture.set_width_request(width);
            picture.set_height_request(height);
            let overlay = page_overlay(&picture);
            pages.append(&overlay);
            (overlay, picture)
        };
        let (landscape, _) = page(WIDE, NARROW);
        let (portrait, portrait_picture) = page(NARROW, WIDE);

        let window = Window::new();
        window.set_default_size(1_200, 900);
        window.set_child(Some(&pages));
        window.present();
        settle(&pages);

        assert_eq!(
            landscape.width(),
            WIDE,
            "the widest page sets the column's width and keeps its own"
        );
        assert_eq!(
            portrait.width(),
            NARROW,
            "the narrow page's overlay must not take the column's width"
        );
        assert_eq!(
            portrait.width(),
            portrait_picture.width(),
            "the overlay and the page drawn in it are one box"
        );

        window.destroy();
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

    /// What the overlay consults to know which values pdfium is drawing for
    /// itself — so it has to describe every field the bytes carry, by the
    /// name the widget in those bytes goes by.
    #[test]
    fn rendered_field_values_maps_every_field_by_its_own_name() {
        let mut document = Document::blank();
        let mut filled = a_form_field(0);
        filled.name = "Name".to_string();
        filled.value = FieldValue::Text("Ada".to_string());
        document.form_fields.insert(filled);
        document.form_fields.insert(a_form_field(1));

        let rendered = rendered_field_values(Some(&document));

        assert_eq!(rendered.len(), 2);
        assert_eq!(
            rendered.get("Name"),
            Some(&FieldValue::Text("Ada".to_string()))
        );
    }

    /// No model means no handle worth describing, and an empty map is the
    /// answer that makes every field the overlay's — the same "nothing to
    /// defer to" default `backend_page_order` takes.
    #[test]
    fn rendered_field_values_is_empty_without_a_model() {
        assert!(rendered_field_values(None).is_empty());
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
}
