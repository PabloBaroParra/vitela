//! Document lifecycle: the file chooser, the generation-guarded open flow,
//! the encrypted-document password prompt, and background close.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::{
    gio, glib, ApplicationWindow, Dialog, Entry, FileChooserAction, FileChooserNative, FileFilter,
    Label, Picture, ResponseType,
};
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderError};

use super::layout::set_placeholder_size;
use super::render::update_viewport;
use super::search::update_search_controls;
use super::state::{DocumentSession, FitRequest, OpenedDocument, PageSlot, PageState, Viewer};

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
            open_initial(&window, &viewer, path);
        }
    });
    chooser.show();
    active_chooser.replace(Some(chooser));
}

fn open_initial(window: &ApplicationWindow, viewer: &Viewer, path: PathBuf) {
    let generation = begin_loading(viewer);
    viewer.status.set_text("Opening PDF...");
    glib::spawn_future_local({
        let window = window.clone();
        let viewer = viewer.clone();
        async move {
            match open_in_background(path.clone(), None).await {
                Ok(document) if is_current(&viewer, generation) => {
                    show_document(&viewer, generation, document);
                }
                Ok(document) => close_document_in_background(document.document),
                Err(RenderError::InvalidPassword) if is_current(&viewer, generation) => {
                    prompt_for_password(&window, &viewer, path, generation);
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
    let mut state = viewer.state.borrow_mut();
    state.generation += 1;
    state.generation
}

pub(crate) fn is_current(viewer: &Viewer, generation: u64) -> bool {
    viewer.state.borrow().generation == generation
}

async fn open_in_background(
    path: PathBuf,
    password: Option<String>,
) -> Result<OpenedDocument, RenderError> {
    gio::spawn_blocking(move || open_document(&path, password.as_deref()))
        .await
        .expect("document-open task panicked")
}

fn open_document(path: &Path, password: Option<&str>) -> Result<OpenedDocument, RenderError> {
    let renderer = PdfiumRenderer::new();
    let document = renderer.open_document(path, password)?;
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
            close_document_in_background(session.document);
        }
    }
    while let Some(child) = viewer.pages.first_child() {
        viewer.pages.remove(&child);
    }

    let fit = FitRequest::measure(viewer);
    let mut slots = Vec::with_capacity(document.page_sizes.len());
    let mut page_heights = Vec::with_capacity(document.page_sizes.len());
    for (width_pt, height_pt) in document.page_sizes {
        let picture = Picture::new();
        picture.set_can_shrink(true);
        picture.set_keep_aspect_ratio(true);
        let logical_height = set_placeholder_size(&picture, width_pt, height_pt, fit);
        viewer.pages.append(&picture);
        slots.push(PageSlot {
            picture,
            width_pt,
            height_pt,
            state: PageState::Idle,
        });
        page_heights.push(logical_height);
    }

    let page_count = slots.len();
    viewer.state.borrow_mut().session = Some(DocumentSession {
        document: document.document,
        physical_width: fit.available_width,
        scale_factor: fit.scale_factor,
        pages: slots,
        page_heights,
        last_visible: None,
        search: None,
        next_search_id: 0,
        active: HashMap::new(),
        next_render_id: 0,
    });
    // A new document invalidates the previous document's matches.
    update_search_controls(viewer);
    viewer.print_button.set_sensitive(page_count > 0);
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
    path: PathBuf,
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

    let password_entry = Entry::builder()
        .visibility(false)
        .activates_default(true)
        .build();
    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    dialog.content_area().append(&password_entry);
    dialog.content_area().append(&error_label);
    password_entry.grab_focus();

    dialog.connect_response({
        let viewer = viewer.clone();
        move |dialog, response| {
            if response != ResponseType::Accept {
                viewer.status.set_text("Password entry cancelled.");
                dialog.close();
                return;
            }

            viewer.status.set_text("Opening password-protected PDF...");
            dialog.set_sensitive(false);
            glib::spawn_future_local({
                let dialog = dialog.clone();
                let viewer = viewer.clone();
                let password_entry = password_entry.clone();
                let error_label = error_label.clone();
                let path = path.clone();
                async move {
                    let password = password_entry.text().to_string();
                    match open_in_background(path, Some(password)).await {
                        Ok(document) if is_current(&viewer, generation) => {
                            show_document(&viewer, generation, document);
                            dialog.close();
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
                            dialog.close();
                        }
                        Err(_) => {}
                    }
                }
            });
        }
    });
    dialog.present();
}
