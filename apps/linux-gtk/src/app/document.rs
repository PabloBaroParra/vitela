//! Document lifecycle: the file chooser, the built-in sample, the
//! queued open flow (each open lands in a tab of its own, see
//! [`super::tabs`]), the encrypted-document password prompt, and background
//! close. Installing a session on screen lives in [`install`].
//!
//! Everything that writes a document out lives in [`super::write`] instead —
//! Save, signing, protecting and the content-edit preview refresh. The two
//! halves meet at [`show_document`], which every one of those chains reaches
//! to install the document it reopened, and at the unsaved-changes prompt
//! below, which is the one place lifecycle has to run a save itself.

mod install;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gio, glib, AlertDialog, ApplicationWindow, Box as GtkBox, Button, FileDialog, Label,
    Orientation as GtkOrientation, Overlay, PasswordEntry, Picture, Window,
};
use pdf_document::{Document, FieldValue, Orientation, PageSize, SecurityContext};
use pdf_manip::ManipError;
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderError};

pub(crate) use install::{
    build_pages, cancel_renders, clear_page_widgets, install_session, parked_session,
    retire_active_session, show_document, show_no_document, BuiltPages, PageShape,
};

use super::home::abandon_pending_tool;
use super::i18n::{tr, tr_args};
use super::state::{
    AnnotationAccess, ContentEditAccess, DocumentSource, OpenedDocument, PageAssemblyAccess,
    TextAccess, Viewer,
};
use super::tabs::Placement;
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
        .title(tr("open-chooser-title"))
        .accept_label(tr("open-accept"))
        .default_filter(&filter)
        .build();
    // Several at once: each chosen file opens in a tab of its own.
    // Held until the answer is in — see `tabs::ChooserScope`. Unbound: the
    // files open into tabs of their own, whatever tab is active by then.
    let scope = crate::app::tabs::ChooserScope::unbound(viewer);
    chooser.open_multiple(Some(window), None::<&gio::Cancellable>, {
        let window = window.clone();
        let viewer = viewer.clone();
        move |result| {
            if !scope.settle(&viewer) {
                return;
            }
            let files = result.map(|model| {
                (0..model.n_items())
                    .filter_map(|index| model.item(index).and_downcast::<gio::File>())
                    .collect()
            });
            open_chosen_files(&window, &viewer, files)
        }
    });
}

/// What the file chooser's answer leads to: one tab per local file, and
/// nothing at all for a dismissed chooser or a choice with no local path.
///
/// Split out of [`show_file_chooser`] so the answers that end the open there
/// can be driven without a real dialog.
fn open_chosen_files(
    window: &ApplicationWindow,
    viewer: &Viewer,
    result: Result<Vec<gio::File>, glib::Error>,
) {
    let Ok(files) = result else {
        abandon_pending_tool(viewer);
        return;
    };
    let paths: Vec<PathBuf> = files.iter().filter_map(gio::File::path).collect();
    if paths.len() < files.len() {
        viewer.status.set_text(&tr("open-not-local-file"));
    }
    if paths.is_empty() {
        abandon_pending_tool(viewer);
        return;
    }
    super::tabs::open_paths(window, viewer, paths);
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
pub(crate) fn rendered_field_values(model: Option<&Document>) -> HashMap<String, FieldValue> {
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
    let Some(window) = viewer.window() else {
        viewer.status.set_text(&tr("open-window-unavailable"));
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
        viewer.status.set_text(&tr("open-blank-failed"));
        return;
    };
    let mut bytes = Vec::new();
    if base.as_lopdf().clone().save_to(&mut bytes).is_err() {
        viewer.status.set_text(&tr("open-blank-failed"));
        return;
    }
    open_initial(window, viewer, DocumentSource::Bytes(bytes));
}

/// Every single-document open funnels through here — a chooser pick, a
/// drop, a recents card, the sample, a blank document: one user action, so
/// it may land in front (`tabs::Placement::Front`).
pub(crate) fn open_initial(window: &ApplicationWindow, viewer: &Viewer, source: DocumentSource) {
    open_source(window, viewer, source, Placement::Front);
}

/// Opens `source` in a tab of its own, so nothing on screen is replaced and
/// there is no unsaved work to ask about. Opens are queued
/// (`tabs::enqueue_open`), so one never cancels another.
pub(crate) fn open_source(
    window: &ApplicationWindow,
    viewer: &Viewer,
    source: DocumentSource,
    placement: Placement,
) {
    super::tabs::enqueue_open(
        window,
        viewer,
        Box::new(move |window: &ApplicationWindow, viewer: &Viewer| {
            begin_open(window, viewer, source, placement)
        }),
    );
}

/// An open's turn: a file that is already open in a tab — perhaps opened
/// by an earlier open of this same queue — is not opened twice.
fn begin_open(
    window: &ApplicationWindow,
    viewer: &Viewer,
    source: DocumentSource,
    placement: Placement,
) {
    let placement = match (&source, placement) {
        // A reload was decided when it was queued; read it as it is.
        (_, Placement::Reload(_) | Placement::ReloadOverUnsaved(_)) => placement,
        (DocumentSource::File(path), _) => {
            match super::tabs::activate_open_path(viewer, path, placement) {
                super::tabs::AlreadyOpen::No => placement,
                super::tabs::AlreadyOpen::Reload(holder) => Placement::Reload(holder),
                super::tabs::AlreadyOpen::Handled => {
                    super::tabs::finish_open(window, viewer);
                    return;
                }
                super::tabs::AlreadyOpen::AskThenReload(holder) => {
                    // Asked (holding the tab) before this open ends; the answer
                    // queues the reload as an open of its own.
                    ask_then_reload(window, viewer, path.clone(), holder);
                    super::tabs::finish_open(window, viewer);
                    return;
                }
            }
        }
        _ => placement,
    };
    start_open(window, viewer, source, placement);
}

/// The file on screen, opened again over unsaved work: the question closing
/// its tab would ask — Save, Discard or Cancel — then, unless cancelled, a
/// reload from disk into the same tab. Save writes the work first. Discard
/// changes nothing yet: the work is replaced — and its recovery entry let go —
/// only when the reload lands; one that is dropped leaves it as it was.
fn ask_then_reload(
    window: &ApplicationWindow,
    viewer: &Viewer,
    path: PathBuf,
    holder: super::tabs::TabId,
) {
    confirm_unsaved_edits(window, viewer, UNSAVED_DETAIL_RELOADING, {
        let window = window.clone();
        let viewer = viewer.clone();
        move || {
            if super::tabs::active_tab(&viewer) != Some(holder) {
                viewer.status.set_text(&tr("tabs-document-changed"));
                return;
            }
            open_source(
                &window,
                &viewer,
                DocumentSource::File(path.clone()),
                Placement::ReloadOverUnsaved(holder),
            );
        }
    });
}

/// Stops a close request while unsaved work is being resolved.
///
/// The signal itself must return synchronously, while both the confirmation
/// and save flows are asynchronous. A dirty window therefore stays alive
/// until every dirty tab has been saved or explicitly discarded — see
/// `tabs::close::resolve_dirty_tabs_then`.
pub(crate) fn confirm_closing_edits(
    window: &ApplicationWindow,
    viewer: &Viewer,
) -> glib::Propagation {
    if !has_unsaved_changes_after_commit(viewer) && !super::tabs::has_dirty_parked_tab(viewer) {
        return glib::Propagation::Proceed;
    }

    let window_to_close = window.clone();
    super::tabs::close::resolve_dirty_tabs_then(window, viewer, move || window_to_close.destroy());
    glib::Propagation::Stop
}

/// The unsaved-changes detail when the window is about to close.
pub(crate) const UNSAVED_DETAIL_CLOSING: &str = "open-unsaved-detail-closing";
/// The unsaved-changes detail when the file on screen is about to be read
/// again from disk.
const UNSAVED_DETAIL_RELOADING: &str = "tabs-unsaved-detail-reloading";

/// Save, Discard or Cancel for the active document's unsaved work, then
/// `proceed` — on Discard at once, on Save only once the bytes reached disk.
///
/// Cancel is every exit that is not a deliberate choice: the button, Escape,
/// and closing the window. Work is lost only when someone picks Discard.
///
/// `detail_key` names the whole detail paragraph rather than its second
/// sentence: two sentences joined in code would fix their order and spacing
/// for every language.
pub(crate) fn confirm_unsaved_edits(
    window: &ApplicationWindow,
    viewer: &Viewer,
    detail_key: &'static str,
    proceed: impl Fn() + 'static,
) {
    let dialog = AlertDialog::builder()
        .message(tr("open-unsaved-title"))
        .detail(tr(detail_key))
        .buttons([
            tr("open-unsaved-cancel"),
            tr("open-unsaved-discard"),
            tr("open-unsaved-save"),
        ])
        .cancel_button(0)
        .default_button(2)
        .modal(true)
        .build();

    let proceed: Rc<dyn Fn()> = Rc::new(proceed);
    // Holds the tab being asked about on screen until the answer is in — an
    // open landing meanwhile would otherwise park it under the dialog.
    let guard = super::tabs::PromptGuard::hold(viewer);
    dialog.choose(Some(window), None::<&gio::Cancellable>, {
        let viewer = viewer.clone();
        let window = window.clone();
        move |response| {
            // Let go before acting: the answer may itself switch tabs.
            drop(guard);
            match unsaved_decision(response.ok()) {
                UnsavedDecision::Discard => proceed(),
                UnsavedDecision::Save => {
                    save_then(&window, &viewer, proceed.clone());
                }
                UnsavedDecision::Keep => viewer.status.set_text(&tr("open-unsaved-kept")),
            }
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
/// Save precisely to keep the work, and closing anyway would discard exactly
/// what they asked to preserve.
fn save_then(window: &ApplicationWindow, viewer: &Viewer, after_save: Rc<dyn Fn()>) {
    super::write::show_save_chooser_then(window, viewer, Some(after_save));
}

fn start_open(
    window: &ApplicationWindow,
    viewer: &Viewer,
    source: DocumentSource,
    placement: Placement,
) {
    viewer.status.set_text(&tr("open-busy"));
    glib::spawn_future_local({
        let window = window.clone();
        let viewer = viewer.clone();
        async move {
            match open_in_background(source.clone(), None).await {
                Ok(document) => land_opened(&window, &viewer, document, source, placement),
                Err(RenderError::InvalidPassword) => {
                    prompt_for_password(&window, &viewer, source, placement);
                }
                Err(error) => {
                    viewer.status.set_text(&open_failed(&error));
                    abandon_pending_tool(&viewer);
                    super::tabs::finish_open(&window, &viewer);
                }
            }
        }
    });
}

/// Puts an opened document in a new tab — once the active one can be parked
/// (`tabs::land_in_new_tab`) — then registers it as recent and ends the
/// open, which starts the next queued one.
fn land_opened(
    window: &ApplicationWindow,
    viewer: &Viewer,
    document: OpenedDocument,
    source: DocumentSource,
    placement: Placement,
) {
    let window = window.clone();
    super::tabs::land_in_new_tab(
        viewer,
        document,
        placement,
        Box::new(move |viewer: &Viewer, _: Option<super::tabs::TabId>| {
            remember_recent(&source);
            super::tabs::finish_open(&window, viewer);
        }),
    );
}

/// The status line for an open that failed. `error` is the renderer's own
/// detail and stays as it is, untranslated.
fn open_failed(error: &impl std::fmt::Display) -> String {
    tr_args("open-failed", &[("error", error.to_string().into())])
}

/// The page-navigation button's tooltip and accessible name.
fn go_to_page(page_number: usize) -> String {
    tr_args("open-go-to-page", &[("page", page_number.into())])
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

/// Bumps the session generation the way a reopen does and returns it, so a
/// test can hand [`show_document`] a current generation without going
/// through an open — the way the recovery tests install a document.
#[cfg(test)]
pub(crate) fn begin_loading(viewer: &Viewer) -> u64 {
    let mut state = viewer.state.borrow_mut();
    state.generation += 1;
    state.generation
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
            origin_path: match source {
                DocumentSource::File(path) => Some(path.clone()),
                DocumentSource::Embedded(_) | DocumentSource::Bytes(_) => None,
            },
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
///
/// The fallbacks are localized: the name is shown on Organize's document card
/// and becomes the stem of exported and split file names. This runs on the
/// open worker thread, whose language comes from the same environment as the
/// main thread's (see `i18n`), so both agree.
fn source_name(source: &DocumentSource) -> String {
    match source {
        DocumentSource::File(path) => path
            .file_name()
            .and_then(|name| name.to_str())
            .map_or_else(|| tr("open-name-fallback"), str::to_owned),
        DocumentSource::Embedded(_) => tr("open-name-sample"),
        DocumentSource::Bytes(_) => tr("open-name-untitled"),
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
/// **This is the only source of permissions in this shell.** It is the cheap
/// half of an open — no decrypting load — and it still answers when lopdf can
/// read the security policy but not build a model. A document whose *user
/// password is empty* ("opens with no prompt, yet still restricts", the most
/// common shape of restricted PDF) is unlocked in place by lopdf, which drops
/// `/Encrypt` from the trailer; the probe recovers its permissions from the
/// decoded encryption state, and `pdf_manip::open_document` now reports the
/// same context. `core/pdf-manip/tests/empty_user_password_open.rs` pins that
/// the two agree.
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
/// model comes from the probe, not from the loader, so this shell keeps one
/// source of permissions (see [`read_security_context`]).
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
            base: base.into(),
            original_bytes: original_bytes.into(),
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

/// [`has_unsaved_changes`], once an open retype box has been committed.
///
/// The box is work too, but it is not in the model until it is committed,
/// so `unsaved_to_disk` alone cannot see it. Committed first, it counts like
/// any other edit. One the core refuses stays open with its text and counts
/// as unsaved by itself: the prompt is what stops that text going with the
/// document, and only Discard gives it up.
pub(crate) fn has_unsaved_changes_after_commit(viewer: &Viewer) -> bool {
    !crate::app::content_edit::editor::commit_open_retype(viewer) || has_unsaved_changes(viewer)
}

/// Whether the active tab's document carries changes that have not been
/// written to disk. Asked before that tab is closed, and for each dirty tab
/// when the window closes (`tabs::close`).
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
    placement: Placement,
) {
    let content = GtkBox::new(GtkOrientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let dialog = Window::builder()
        .transient_for(window)
        .modal(true)
        .title(tr("open-password-title"))
        .child(&content)
        .build();

    let password_entry = PasswordEntry::builder().show_peek_icon(true).build();
    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    let buttons = GtkBox::new(GtkOrientation::Horizontal, 8);
    let cancel = Button::with_label(&tr("open-password-cancel"));
    let open = Button::with_label(&tr("open-password-open"));
    buttons.append(&cancel);
    buttons.append(&open);
    content.append(&password_entry);
    content.append(&error_label);
    content.append(&buttons);
    password_entry.grab_focus();

    // Tracked so its own close can tell whether it is still the current
    // prompt — see `dismiss_password_dialog`. Opens are queued, so there is
    // never more than one.
    viewer.state.borrow_mut().password_dialog = Some(dialog.clone());

    let submit: Rc<dyn Fn()> = Rc::new({
        let window = window.clone();
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        let password_entry = password_entry.clone();
        let error_label = error_label.clone();
        let source = source.clone();
        move || {
            viewer.status.set_text(&tr("open-password-busy"));
            dialog.set_sensitive(false);
            glib::spawn_future_local({
                let window = window.clone();
                let dialog = dialog.clone();
                let viewer = viewer.clone();
                let password_entry = password_entry.clone();
                let error_label = error_label.clone();
                let source = source.clone();
                async move {
                    let password = password_entry.text().to_string();
                    let opened = open_in_background(source.clone(), Some(password)).await;
                    // Closed while the password was being tried: the open was
                    // abandoned — and already ended by the close — so the answer
                    // is dropped, the document with it.
                    if viewer.state.borrow().password_dialog.as_ref() != Some(&dialog) {
                        if let Ok(document) = opened {
                            close_document_in_background(document.document);
                        }
                        return;
                    }
                    match opened {
                        Ok(document) => {
                            // See the comment on the cancel branch below:
                            // `close` would re-emit `response(DeleteEvent)`
                            // and stomp the status the landing sets.
                            dismiss_password_dialog(&viewer, &dialog);
                            land_opened(&window, &viewer, document, source, placement);
                        }
                        Err(RenderError::InvalidPassword) => {
                            dialog.set_sensitive(true);
                            viewer.status.set_text(&tr("open-password-waiting"));
                            error_label.set_text(&tr("open-password-wrong"));
                            password_entry.set_text("");
                            password_entry.grab_focus();
                        }
                        Err(error) => {
                            viewer.status.set_text(&open_failed(&error));
                            abandon_pending_tool(&viewer);
                            dismiss_password_dialog(&viewer, &dialog);
                            super::tabs::finish_open(&window, &viewer);
                        }
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
        let window = window.clone();
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        move |_| {
            viewer.status.set_text(&tr("open-password-cancelled"));
            abandon_pending_tool(&viewer);
            dismiss_password_dialog(&viewer, &dialog);
            super::tabs::finish_open(&window, &viewer);
        }
    });
    // The title bar's close button abandons the open as surely as Cancel.
    // `destroy` — how Cancel and a landed or failed open take the prompt
    // down — does not emit `close-request`, so this sees only the user
    // closing it.
    dialog.connect_close_request({
        let window = window.clone();
        let viewer = viewer.clone();
        let dialog = dialog.clone();
        move |_| {
            if viewer.state.borrow().password_dialog.as_ref() == Some(&dialog) {
                viewer.state.borrow_mut().password_dialog = None;
                viewer.status.set_text(&tr("open-password-cancelled"));
                abandon_pending_tool(&viewer);
                super::tabs::finish_open(&window, &viewer);
            }
            glib::Propagation::Proceed
        }
    });
    dialog.present();
}

/// Clears the tracked password dialog and tears it down — but only if it
/// still points at `dialog`. A test, or the window's teardown, may have
/// replaced or destroyed this same dialog already; comparing
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
        confirm_closing_edits, go_to_page, next_form_field_id, open_chosen_files, open_failed,
        open_initial, page_overlay, prompt_for_password, rendered_field_values, source_name,
        unsaved_decision, Placement, UnsavedDecision, AES128_SAMPLE_PDF, SAMPLE_PDF,
        UNSAVED_DETAIL_CLOSING,
    };
    use crate::app::content_edit::test_support::OpenRetype;
    use crate::app::i18n::{speak, tr, tr_args, Language};
    use crate::app::state::DocumentSource;
    use crate::app::state::HomeTool;
    use crate::app::test_fixtures::a_form_field;
    use crate::app::ui_tests::built_ui;
    use crate::app::BuiltUi;
    use gtk::prelude::*;
    use gtk::{gio, glib, Box as GtkBox, Button, ContentFit, Orientation, Picture, Window};
    use pdf_document::{Document, FieldValue};
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

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

    /// Every sentence the open workflow puts on screen, in the order the
    /// workflow reaches them: chooser, unsaved-changes prompt, progress and
    /// results, page navigation, password prompt. The renderer's own error
    /// detail is an argument and must come through verbatim.
    fn workflow_text() -> Vec<String> {
        let mut text: Vec<String> = [
            "open-chooser-title",
            "open-accept",
            "open-not-local-file",
            "open-window-unavailable",
            "open-blank-failed",
            "open-unsaved-title",
            UNSAVED_DETAIL_CLOSING,
            "open-unsaved-cancel",
            "open-unsaved-discard",
            "open-unsaved-save",
            "open-unsaved-kept",
            "open-busy",
            "open-no-pages",
            "open-password-title",
            "open-password-cancel",
            "open-password-open",
            "open-password-busy",
            "open-password-waiting",
            "open-password-wrong",
            "open-password-cancelled",
        ]
        .map(tr)
        .into();
        text.push(open_failed(&"PDFium: {detail}"));
        text.push(go_to_page(12));
        text.extend(
            [
                DocumentSource::File(PathBuf::from("report.pdf")),
                DocumentSource::Embedded(SAMPLE_PDF),
                DocumentSource::Bytes(Vec::new()),
            ]
            .iter()
            .map(source_name),
        );
        text
    }

    /// The workflow as it read before it moved into the catalogs — byte for
    /// byte, so an English user sees no change.
    #[test]
    fn the_open_workflow_keeps_its_english() {
        assert_eq!(
            workflow_text(),
            [
                "Open PDF",
                "Open",
                "The selected location is not a local file.",
                "The application window is unavailable.",
                "Could not create a blank PDF.",
                "Unsaved changes",
                "The open document has changes that are not saved. Closing Vitela will \
                 discard them.",
                "Cancel",
                "Discard",
                "Save",
                "Kept the unsaved changes.",
                "Opening PDF...",
                "The PDF contains no pages.",
                "Password required",
                "Cancel",
                "Open",
                "Opening password-protected PDF...",
                "Waiting for the document password.",
                "The password is incorrect. Try again.",
                "Password entry cancelled.",
                "Could not open PDF: PDFium: {detail}",
                "Go to page 12",
                "report.pdf",
                "Sample document",
                "Untitled document",
            ]
        );
    }

    #[test]
    fn the_open_workflow_speaks_spanish() {
        let _spanish = speak(Language::Spanish);

        assert_eq!(
            workflow_text(),
            [
                "Abrir PDF",
                "Abrir",
                "La ubicación seleccionada no es un archivo local.",
                "La ventana de la aplicación no está disponible.",
                "No se pudo crear un PDF en blanco.",
                "Cambios sin guardar",
                "El documento abierto tiene cambios que no se han guardado. Si cierras \
                 Vitela, se descartarán.",
                "Cancelar",
                "Descartar",
                "Guardar",
                "Se han conservado los cambios sin guardar.",
                "Abriendo el PDF...",
                "El PDF no contiene páginas.",
                "Se necesita una contraseña",
                "Cancelar",
                "Abrir",
                "Abriendo el PDF protegido con contraseña...",
                "Esperando la contraseña del documento.",
                "La contraseña no es correcta. Inténtalo de nuevo.",
                "Se canceló la contraseña.",
                "No se pudo abrir el PDF: PDFium: {detail}",
                "Ir a la página 12",
                // A real file name is the user's, in any language.
                "report.pdf",
                "Documento de ejemplo",
                "Documento sin título",
            ]
        );
    }

    /// The fallback for a file name that is not valid UTF-8 — the one
    /// `source_name` branch a plain path cannot reach.
    #[cfg(unix)]
    #[test]
    fn a_file_name_that_is_not_unicode_falls_back_to_a_localized_name() {
        use std::os::unix::ffi::OsStrExt;
        let source =
            DocumentSource::File(PathBuf::from(std::ffi::OsStr::from_bytes(b"bad-\xff.pdf")));

        assert_eq!(source_name(&source), "Opened PDF");
        let _spanish = speak(Language::Spanish);
        assert_eq!(source_name(&source), "PDF abierto");
    }

    /// The shared wait — see `test_support`.
    use crate::app::test_support::wait_until;

    /// The first button under `root` labelled `label`, depth first.
    fn find_button(root: &gtk::Widget, label: &str) -> Option<Button> {
        if let Some(button) = root.downcast_ref::<Button>() {
            if button.label().as_deref() == Some(label) {
                return Some(button.clone());
            }
        }
        let mut child = root.first_child();
        while let Some(widget) = child {
            if let Some(found) = find_button(&widget, label) {
                return Some(found);
            }
            child = widget.next_sibling();
        }
        None
    }

    /// The password prompt for an encrypted sample, with Sign armed the way a
    /// Home tile or a mode arms it before there is a document.
    ///
    /// Raised directly rather than through `open_initial`: what these tests
    /// exercise is the prompt's own exits, which pdfium plays no part in, so
    /// they need not wait on a background open to report the password.
    fn prompting_for_a_password(built: &BuiltUi) -> Window {
        built.viewer.state.borrow_mut().pending_tool = Some(HomeTool::Sign);
        prompt_for_password(
            &built.window,
            &built.viewer,
            DocumentSource::Embedded(AES128_SAMPLE_PDF),
            Placement::Front,
        );
        let dialog = built.viewer.state.borrow().password_dialog.clone();
        dialog.expect("the prompt is tracked")
    }

    /// An open box is work on the active document, and an open never takes
    /// that document away from it: the box is committed — its text reaches
    /// the model — and since the commit's preview refresh then holds the tab,
    /// the new document opens in the background. Nothing asks.
    #[gtk::test]
    fn gtk_ui_opening_another_document_commits_the_open_retype_and_opens_behind_it() {
        let window = OpenRetype::new();
        assert!(window.box_open(), "the fixture must leave the box open");

        open_initial(
            window.window(),
            window.viewer(),
            DocumentSource::Embedded(SAMPLE_PDF),
        );
        assert!(
            wait_until(|| window.viewer().tabs.borrow().parked_count() == 1),
            "the new document must open in a tab of its own"
        );
        window.settle();

        assert!(window.dialogs().is_empty(), "opening asks nothing");
        assert!(!window.box_open(), "the box is committed");
        assert_eq!(window.saved_texts(), [OpenRetype::TYPED]);
    }

    /// A box the core refuses keeps its text and its document in front: every
    /// open made meanwhile lands in the background, none is dropped, and the
    /// queue drains.
    #[gtk::test]
    fn gtk_ui_opens_over_a_refused_retype_land_in_the_background() {
        let window = OpenRetype::typing("Kőszeg");
        let said = crate::app::test_support::status_log(window.viewer());

        for _ in 0..3 {
            open_initial(
                window.window(),
                window.viewer(),
                DocumentSource::Embedded(SAMPLE_PDF),
            );
        }

        assert!(
            wait_until(|| window.viewer().tabs.borrow().parked_count() == 3),
            "every open lands: {}",
            window.viewer().status.text()
        );
        assert!(window.box_open(), "the refused box stays open");
        assert!(!window.viewer().tabs.borrow().is_opening());
        let opened = tr_args(
            "tabs-opened-in-background",
            &[("name", tr("open-name-sample").into())],
        );
        assert!(said.borrow().contains(&opened), "{:?}", said.borrow());
    }

    /// Discard over a refused retype box: the file is read again in its place.
    #[gtk::test]
    fn gtk_ui_discarding_over_a_refused_retype_reloads_the_file() {
        let window = OpenRetype::typing("Kőszeg");
        let path = std::env::temp_dir().join(format!("vitela-refused-{}.pdf", std::process::id()));
        std::fs::write(&path, SAMPLE_PDF).unwrap();
        if let Some(session) = window.viewer().state.borrow_mut().session.as_mut() {
            session.origin_path = Some(path.clone());
        }
        let (viewer, source) = (window.viewer(), DocumentSource::File(path.clone()));
        let handle = || viewer.state.borrow().session.as_ref().map(|s| s.document);
        let before = handle();

        open_initial(window.window(), viewer, source);
        assert!(wait_until(|| window.dialogs().len() == 1));
        let discard = tr("open-unsaved-discard");
        let dialogs = window.dialogs();
        let button = dialogs
            .iter()
            .find_map(|d| find_button(d.upcast_ref(), &discard));
        button.expect("Discard").emit_clicked();
        let reloaded = wait_until(|| handle() != before);
        let _ = std::fs::remove_file(&path);

        assert!(reloaded, "{}", window.viewer().status.text());
        assert!(!window.box_open());
        assert!(!window.unsaved_to_disk());
    }

    /// Closing the window: the same commit, and the close is held for the
    /// prompt.
    #[gtk::test]
    fn gtk_ui_closing_the_window_commits_the_open_retype_and_asks() {
        let window = OpenRetype::new();
        assert!(!window.unsaved_to_disk(), "only the box holds unsaved work");

        let propagation = confirm_closing_edits(window.window(), window.viewer());
        window.settle();

        assert_eq!(propagation, glib::Propagation::Stop);
        assert!(!window.box_open(), "the close commits the box");
        assert_eq!(window.saved_texts(), [OpenRetype::TYPED]);
        assert_eq!(window.dialogs().len(), 1, "the unsaved-changes prompt");
    }

    /// A box the core refuses cannot be committed, and nothing else marks
    /// the document dirty — the box itself is the unsaved work, so the close
    /// still asks rather than dropping the text.
    #[gtk::test]
    fn gtk_ui_closing_over_a_refused_retype_still_asks() {
        let window = OpenRetype::typing("Kőszeg");

        let propagation = confirm_closing_edits(window.window(), window.viewer());
        window.settle();

        assert_eq!(propagation, glib::Propagation::Stop);
        assert!(window.box_open(), "the refused box stays open");
        assert!(!window.unsaved_to_disk());
        assert_eq!(window.dialogs().len(), 1, "the unsaved-changes prompt");
    }

    /// A chooser the user dismissed is the open the tool was armed for ending
    /// with no document. Left armed, the tool would fire on whatever the user
    /// opened next from anywhere else — Recent, a drop, Ctrl+O.
    #[gtk::test]
    fn gtk_ui_a_dismissed_chooser_drops_the_pending_tool() {
        let built = built_ui();
        built.viewer.state.borrow_mut().pending_tool = Some(HomeTool::Sign);

        open_chosen_files(
            &built.window,
            &built.viewer,
            Err(glib::Error::new(gtk::DialogError::Dismissed, "dismissed")),
        );

        assert_eq!(built.viewer.state.borrow().pending_tool, None);
        built.window.close();
    }

    /// A file with no local path never starts an open, so it ends the one the
    /// tool was armed for just as a dismissed chooser does.
    #[gtk::test]
    fn gtk_ui_a_chosen_file_with_no_local_path_drops_the_pending_tool() {
        let built = built_ui();
        built.viewer.state.borrow_mut().pending_tool = Some(HomeTool::Sign);
        // An unknown scheme gives GIO's placeholder file, which has no path.
        let remote = gio::File::for_uri("vitela-test://nowhere/file.pdf");
        assert!(remote.path().is_none(), "the fixture must not be local");

        open_chosen_files(&built.window, &built.viewer, Ok(vec![remote]));

        assert_eq!(built.viewer.status.text(), tr("open-not-local-file"));
        assert_eq!(built.viewer.state.borrow().pending_tool, None);
        built.window.close();
    }

    /// An open that fails — here, bytes that are not a PDF — ends without a
    /// document, so it must not leave the tool for the next one.
    #[gtk::test]
    fn gtk_ui_a_failed_open_drops_the_pending_tool() {
        let built = built_ui();
        built.viewer.state.borrow_mut().pending_tool = Some(HomeTool::Sign);

        open_initial(
            &built.window,
            &built.viewer,
            DocumentSource::Bytes(b"this is not a PDF".to_vec()),
        );

        assert!(
            wait_until(|| built.viewer.status.text() != tr("open-busy")),
            "the open must finish"
        );
        assert!(built.viewer.state.borrow().session.is_none());
        assert_eq!(built.viewer.state.borrow().pending_tool, None);
        built.window.close();
    }

    /// Cancelling the password prompt is the user abandoning the open.
    #[gtk::test]
    fn gtk_ui_cancelling_the_password_prompt_drops_the_pending_tool() {
        let built = built_ui();
        let dialog = prompting_for_a_password(&built);
        let cancel = find_button(dialog.upcast_ref(), &tr("open-password-cancel"))
            .expect("the prompt offers Cancel");

        cancel.emit_clicked();

        assert_eq!(built.viewer.state.borrow().pending_tool, None);
        built.window.close();
    }

    /// Closing the prompt from its title bar abandons the open as surely as
    /// Cancel does, and has to drop the tool the same way.
    #[gtk::test]
    fn gtk_ui_closing_the_password_prompt_drops_the_pending_tool() {
        let built = built_ui();
        let dialog = prompting_for_a_password(&built);

        dialog.close();

        assert_eq!(built.viewer.state.borrow().pending_tool, None);
        built.window.close();
    }

    /// A prompt closed while the password it was given is being tried has
    /// abandoned the open: the document must not open anyway, and the open
    /// ends exactly once.
    #[gtk::test]
    fn gtk_ui_a_password_prompt_closed_during_its_load_opens_nothing() {
        let built = built_ui();
        crate::app::tabs::enqueue_open(&built.window, &built.viewer, Box::new(|_, _| {}));
        let dialog = prompting_for_a_password(&built);
        let entry = {
            let mut found = None;
            let mut stack = vec![dialog.clone().upcast::<gtk::Widget>()];
            while let Some(widget) = stack.pop() {
                if let Some(entry) = widget.downcast_ref::<gtk::PasswordEntry>() {
                    found = Some(entry.clone());
                }
                let mut child = widget.first_child();
                while let Some(next) = child {
                    child = next.next_sibling();
                    stack.push(next);
                }
            }
            found.expect("the password entry")
        };
        entry.set_text("user-aes-pass");
        find_button(dialog.upcast_ref(), &tr("open-password-open"))
            .expect("the prompt offers Open")
            .emit_clicked();
        glib::MainContext::default().iteration(false);
        dialog.close();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            let context = glib::MainContext::default();
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(
            built.viewer.state.borrow().session.is_none(),
            "nothing opened"
        );
        assert!(!built.viewer.tabs.borrow().is_opening(), "the open ended");
        assert_eq!(built.viewer.status.text(), tr("open-password-cancelled"));
        built.window.close();
    }

    /// A wrong password keeps the prompt up for another try, so the open is
    /// still going and the tool must survive it. Needs pdfium to refuse the
    /// password; the GTK UI CI job provides it.
    #[gtk::test]
    fn gtk_ui_a_wrong_password_keeps_the_pending_tool_for_the_retry() {
        let built = built_ui();
        let dialog = prompting_for_a_password(&built);
        let open = find_button(dialog.upcast_ref(), &tr("open-password-open"))
            .expect("the prompt offers Open");

        open.emit_clicked();

        assert!(
            wait_until(|| built.viewer.status.text() == tr("open-password-waiting")),
            "the empty password must be refused"
        );
        assert_eq!(
            built.viewer.state.borrow().pending_tool,
            Some(HomeTool::Sign)
        );
        dialog.destroy();
        built.window.close();
    }
}
