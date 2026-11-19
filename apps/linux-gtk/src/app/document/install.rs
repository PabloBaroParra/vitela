//! Putting a document session on screen — and taking the last one off.
//!
//! Two ways in share one tail. [`show_document`] turns a freshly opened (or
//! reopened) pdfium handle into a new session; a tab switch
//! (`tabs::park`) re-installs a session that was parked with its handle
//! still open. Both rebuild the page widgets with [`build_pages`] and hand
//! the session over through [`install_session`], which is the one place the
//! window's controls are brought in line with whichever document is now
//! active.

use std::collections::HashMap;

use gtk::prelude::*;
use gtk::{Button, ContentFit, Picture};
use pdf_render::PageRotation;

use super::{
    backend_page_order, close_document_in_background, go_to_page, is_current, next_form_field_id,
    page_overlay, rendered_field_values,
};
use crate::app::i18n::tr;
use crate::app::layout::set_placeholder_size;
use crate::app::render::update_viewport;
use crate::app::search::update_search_controls;
use crate::app::state::{DocumentSession, FitRequest, OpenedDocument, PageSlot, PageState, Viewer};

/// A page's drawn size and turn — all the page widgets are built from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PageShape {
    pub(crate) width_pt: f32,
    pub(crate) height_pt: f32,
    pub(crate) rotation: PageRotation,
}

/// The page widgets [`build_pages`] appended, and the measurement they were
/// fitted to.
pub(crate) struct BuiltPages {
    pub(crate) slots: Vec<PageSlot>,
    pub(crate) page_heights: Vec<i32>,
    pub(crate) fit: FitRequest,
}

pub(crate) fn show_document(viewer: &Viewer, generation: u64, document: OpenedDocument) {
    if !is_current(viewer, generation) {
        close_document_in_background(document.document);
        return;
    }

    // Before the measuring below, not after: a document on screen means the
    // editor page, whichever view the open was started from (Home's drop
    // zone, a recents card, Ctrl+O, a file-manager launch), and `FitRequest::
    // measure` reads an allocation the stack only gives a page it is showing.
    // A first open still measures one frame early — `refresh_layout`, wired
    // to the scroller's page-size change, refits as soon as the real
    // allocation lands.
    crate::app::home::show_editor(viewer);

    // The new document opened successfully: only now replace the previous
    // one. Cancel its in-flight renders and close it, then clear its page
    // widgets before building the new layout. A document in another tab is
    // not "the previous one": opening into a new tab parks it first
    // (`tabs::land_in_new_tab`), so there is nothing here to replace.
    retire_active_session(viewer);
    clear_page_widgets(viewer);

    let shapes = shapes_of(&document);
    let built = build_pages(viewer, &shapes);
    let session = new_session(document, built);
    install_session(viewer, session);
    // Last, so a tool armed on Home before there was a document to apply it
    // to lands on controls whose sensitivity `install_session` has already
    // settled for *this* document.
    crate::app::home::apply_pending_tool(viewer);
}

/// A session over `document` with no page widgets yet, and the page shapes
/// to build them from — for a document opened into a tab in the background
/// (`tabs::park`), whose widgets are built when it is first shown.
pub(crate) fn parked_session(
    viewer: &Viewer,
    document: OpenedDocument,
) -> (DocumentSession, Vec<PageShape>) {
    let shapes = shapes_of(&document);
    let unbuilt = BuiltPages {
        slots: Vec::new(),
        page_heights: Vec::new(),
        fit: FitRequest::measure(viewer),
    };
    (new_session(document, unbuilt), shapes)
}

fn shapes_of(document: &OpenedDocument) -> Vec<PageShape> {
    document
        .page_geometry
        .iter()
        .map(|geometry| PageShape {
            width_pt: geometry.width_pt,
            height_pt: geometry.height_pt,
            rotation: geometry.rotation,
        })
        .collect()
}

/// Takes the active session out of the window, cancels its renders and
/// closes its pdfium handle. The page widgets are left to the caller.
pub(crate) fn retire_active_session(viewer: &Viewer) {
    let session = viewer.state.borrow_mut().session.take();
    if let Some(session) = session {
        cancel_renders(&session);
        close_document_in_background(session.document);
    }
}

/// Cancels every page and tile render `session` has in flight.
pub(crate) fn cancel_renders(session: &DocumentSession) {
    for active in session.active.values() {
        active.cancellation.cancel();
    }
    for active in session.active_tiles.values() {
        active.cancellation.cancel();
    }
}

/// Removes every page widget and page-navigation button.
pub(crate) fn clear_page_widgets(viewer: &Viewer) {
    while let Some(child) = viewer.pages.first_child() {
        viewer.pages.remove(&child);
    }
    while let Some(child) = viewer.page_navigation.first_child() {
        viewer.page_navigation.remove(&child);
    }
}

/// Appends one placeholder page widget and one navigation button per shape,
/// fitted to the canvas at fit-width — the zoom a freshly installed session
/// starts at.
pub(crate) fn build_pages(viewer: &Viewer, shapes: &[PageShape]) -> BuiltPages {
    let fit = FitRequest::measure(viewer);
    let mut slots = Vec::with_capacity(shapes.len());
    let mut page_heights = Vec::with_capacity(shapes.len());
    for (page_index, shape) in shapes.iter().enumerate() {
        let (width_pt, height_pt) = (shape.width_pt, shape.height_pt);
        let picture = Picture::new();
        picture.set_can_shrink(true);
        picture.set_content_fit(ContentFit::Contain);
        let logical_height = set_placeholder_size(&picture, width_pt, height_pt, fit);
        let overlay = page_overlay(&picture);
        // Added after the child, so it sits above the rendered page. The tile
        // pipeline slips its tiles in under it (`render::put_tile`).
        let highlights = crate::app::selection::build_highlight_layer(viewer, page_index);
        overlay.add_overlay(&highlights);
        viewer.pages.append(&overlay);
        let page_number = page_index + 1;
        let page_button = Button::with_label(&page_number.to_string());
        let go_to = go_to_page(page_number);
        page_button.set_tooltip_text(Some(&go_to));
        page_button.update_property(&[gtk::accessible::Property::Label(&go_to)]);
        page_button.connect_clicked({
            let viewer = viewer.clone();
            move |_| crate::app::navigate_to_page(&viewer, page_index)
        });
        viewer.page_navigation.append(&page_button);
        let box_ = crate::app::layout::resolve_page_box(
            crate::app::layout::Zoom::FitWidth,
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
            rotation: shape.rotation,
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
    BuiltPages {
        slots,
        page_heights,
        fit,
    }
}

/// A fresh session over `document`, laid out on `built`.
fn new_session(document: OpenedDocument, built: BuiltPages) -> DocumentSession {
    let BuiltPages {
        slots,
        page_heights,
        fit,
    } = built;
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
    DocumentSession {
        document: document.document,
        base_name: document.name,
        origin_path: document.origin_path,
        text_access: document.text_access,
        annotation_access: document.annotation_access,
        content_edit_access: document.content_edit_access,
        page_assembly_access: document.page_assembly_access,
        document_model: document.document_model,
        backend_pages,
        rendered_field_values,
        save_backing: document.save_backing,
        imported_sources: Vec::new(),
        content_checkpoints: Default::default(),
        import_warning_revision: None,
        // Freshly shown: whatever is on screen right now is exactly what
        // this session's bytes came from, which for an ordinary open or a
        // disk-save reopen means it matches disk. A T-163 preview refresh
        // shows bytes that were never written anywhere, so it restores
        // this to `true` right after — see `restore_edit_state`.
        unsaved_to_disk: false,
        unsaved_base: false,
        base_breaks_signatures: false,
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
        selected_text_block: None,
        selected_path: None,
        path_drag: None,
        content_selection: None,
        selection_gesture: None,
        content_pointer: None,
        content_hover: None,
        content_hover_pointer: None,
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
        zoom: crate::app::layout::Zoom::FitWidth,
        zoom_generation: 0,
        active_tiles: HashMap::new(),
    }
}

/// Makes `session` the active document and brings every control in line
/// with it. The page widgets must already be `session.pages`' own. A tool a
/// Home card armed is left to the caller, once the screen it lands on is
/// settled.
///
/// Bumps `session_id`, so anything captured against the previous session —
/// a note dialog, an armed signature, a drop target — is refused when it
/// lands.
pub(crate) fn install_session(viewer: &Viewer, session: DocumentSession) {
    // The Organize thumbnails are pictures of the document being left — see
    // `organize::document_changed` for why a same-length document would
    // otherwise inherit them.
    crate::app::organize::document_changed(viewer);
    let page_count = session.pages.len();
    {
        let mut state = viewer.state.borrow_mut();
        state.session_id += 1;
        state.session = Some(session);
    }
    crate::app::tabs::session_installed(viewer);
    crate::app::metadata::refresh(viewer);
    // A new document invalidates the previous document's matches.
    update_search_controls(viewer);
    crate::app::annotations::update_annotation_controls(viewer);
    crate::app::content_edit::update_controls(viewer);
    crate::app::update_content_edit_controls(viewer);
    crate::app::forms::update_forms_controls(viewer);
    crate::app::sign::update_sign_controls(viewer);
    // The mode outlives the document it was armed on, so a session installed
    // while it is on has to be given the same start `set_mode` would have —
    // see `content_edit::rearm_for_session`.
    crate::app::content_edit::rearm_for_session(viewer);
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
        viewer.status.set_text(&tr("open-no-pages"));
    } else {
        update_viewport(viewer);
    }
}

/// The window with no document at all: what closing the last tab leaves.
///
/// The caller has already retired the session and moved `generation` and
/// `session_id` on (`tabs::close`). This puts the page area and every
/// control back where a freshly built window has them — no annotation tool
/// or selection command, nothing to print, save, export, undo or redo — and
/// the mark back over the empty canvas.
pub(crate) fn show_no_document(viewer: &Viewer) {
    clear_page_widgets(viewer);
    crate::app::organize::document_changed(viewer);
    crate::app::annotations::update_annotation_controls(viewer);
    crate::app::metadata::refresh(viewer);
    update_search_controls(viewer);
    crate::app::content_edit::update_controls(viewer);
    crate::app::update_content_edit_controls(viewer);
    crate::app::forms::update_forms_controls(viewer);
    crate::app::sign::update_sign_controls(viewer);
    viewer.print_button.set_sensitive(false);
    viewer.save_button.set_sensitive(false);
    viewer.export_button.set_sensitive(false);
    viewer.undo_action.set_enabled(false);
    viewer.redo_action.set_enabled(false);
    viewer.app_mark.set_visible(true);
}
