//! Document lifecycle: the file chooser, the built-in sample, the
//! generation-guarded open flow, the encrypted-document password prompt, and
//! background close.

use std::collections::HashMap;
use std::rc::Rc;

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::{
    gio, glib, ApplicationWindow, Dialog, FileChooserAction, FileChooserNative, FileFilter, Label,
    Overlay, PasswordEntry, Picture, ResponseType,
};
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderError};

use super::layout::set_placeholder_size;
use super::render::update_viewport;
use super::search::update_search_controls;
use super::state::{
    DocumentSession, DocumentSource, FitRequest, OpenedDocument, PageSlot, PageState, Viewer,
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

fn open_initial(window: &ApplicationWindow, viewer: &Viewer, source: DocumentSource) {
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
    };
    // One batched actor round-trip for every page size, instead of N
    // serialized `page_size` round-trips — first paint no longer waits on
    // a per-page metadata sweep for large documents.
    match renderer.page_sizes(document, Priority::Visible).wait() {
        Ok(page_sizes) => Ok(OpenedDocument {
            document,
            page_sizes,
        }),
        Err(error) => {
            let _ = renderer.close_document(document);
            Err(error)
        }
    }
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
    viewer.state.borrow_mut().session = Some(DocumentSession {
        document: document.document,
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
    // A new document invalidates the previous document's matches.
    update_search_controls(viewer);
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
