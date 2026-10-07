//! The cheap way to show a preview refresh: swap the pdfium handle under the
//! page widgets already on screen instead of rebuilding them.
//!
//! `document::show_document` tears down and recreates a picture, an overlay,
//! a highlight layer and a navigation button for *every* page. That is the
//! right cost for opening a document and the wrong one for re-showing the
//! same document after one retyped run: on a 1619-page PDF it is thousands
//! of widgets destroyed and built on the main thread per commit, long enough
//! for GNOME to offer "Force Quit". A content edit leaves every page the
//! size it was, so the slots — and with them the zoom, the scroll position
//! and everything the user had edited — can stay exactly where they are.

use pdf_render::PageGeometry;

use crate::app::document::{
    backend_page_order, close_document_in_background, rendered_field_values,
};
use crate::app::state::{OpenedDocument, PageSlot, PageState, Viewer};

/// Installs `reopened`'s handle into the session on screen, keeping its page
/// widgets, its edit state and its view. Hands `reopened` back untouched (`Some`) when
/// the pages are not all the same size as the slots they would be drawn in —
/// a page op changed the layout, and only `show_document` rebuilds that.
///
/// Drops exactly what `show_document` would have dropped because it is keyed
/// to the replaced handle (see `edits::EditState`'s invalidation rule): every
/// page's render, tiles and pdfium text, the text selection and the search
/// matches. A render or text load still in flight against the old handle is
/// discarded by its own `session.document` guard once the handle changes.
///
/// The model-side fields are *not* reinstalled from `reopened`: the session
/// still holds the model, backing and counters the reopen was written from,
/// which is what `edits::restore_edit_state` would have put back anyway.
pub(super) fn swap_in_place(viewer: &Viewer, reopened: OpenedDocument) -> Option<OpenedDocument> {
    {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return Some(reopened);
        };
        if !same_layout(&session.pages, &reopened.page_geometry) {
            return Some(reopened);
        }

        for active in session.active.values() {
            active.cancellation.cancel();
        }
        session.active.clear();
        for active in session.active_tiles.values() {
            active.cancellation.cancel();
        }
        session.active_tiles.clear();
        for page in &mut session.pages {
            reset_slot(page);
        }

        let previous = std::mem::replace(&mut session.document, reopened.document);
        close_document_in_background(previous);
        session.text_access = reopened.text_access;
        session.annotation_access = reopened.annotation_access;
        session.content_edit_access = reopened.content_edit_access;
        session.page_assembly_access = reopened.page_assembly_access;
        // The pdfium pages are in the order the session's own model was
        // written in, and pdfium paints the field values the reopened model
        // read back out of them — the same split `show_document` +
        // `restore_edit_state` make.
        session.backend_pages = backend_page_order(session.document_model.as_ref());
        session.rendered_field_values = rendered_field_values(reopened.document_model.as_ref());
        session.selection = None;
        session.search = None;
        session.unsaved_to_disk = true;
        session.last_visible = None;
    }

    // What `show_document` runs for a new session, minus what only a
    // different document needs (metadata, the Home tool hand-off).
    crate::app::organize::document_changed(viewer);
    crate::app::search::update_search_controls(viewer);
    crate::app::annotations::update_annotation_controls(viewer);
    crate::app::content_edit::update_controls(viewer);
    crate::app::update_content_edit_controls(viewer);
    crate::app::forms::update_forms_controls(viewer);
    crate::app::sign::update_sign_controls(viewer);
    crate::app::render::update_viewport(viewer);
    None
}

/// Whether every page of the reopened document is drawn at the size and turn
/// of the slot already holding it — the condition for keeping the slots.
fn same_layout(pages: &[PageSlot], geometry: &[PageGeometry]) -> bool {
    pages.len() == geometry.len()
        && pages.iter().zip(geometry).all(|(page, geometry)| {
            page.width_pt == geometry.width_pt
                && page.height_pt == geometry.height_pt
                && page.rotation == geometry.rotation
        })
}

/// Forgets everything a slot learned from the old handle.
///
/// The rendered bitmap is *kept* on screen and the page marked `Idle`, so
/// `update_viewport` re-renders it from the new handle and the old picture
/// stands in until then — clearing it first would flash every visible page
/// blank on each commit. `update_viewport` evicts a kept bitmap the same way
/// it evicts a rendered one once its page leaves the cache window.
fn reset_slot(page: &mut PageSlot) {
    if matches!(page.state, PageState::Rendered | PageState::Failed) {
        page.state = PageState::Idle;
    }
    for tile in std::mem::take(&mut page.tiles).into_values() {
        page.overlay.remove_overlay(&tile);
    }
    page.tile_dpi = 0;
    page.tile_generation += 1;
    page.tile_failed_dpi = 0;
    page.characters = None;
    page.characters_requested = false;
    page.content = None;
}

#[cfg(test)]
mod tests {
    use super::{reset_slot, same_layout};
    use crate::app::state::{PageSlot, PageState};
    use gtk::{gdk_pixbuf, Overlay, Picture};
    use pdf_document::PageContent;
    use pdf_render::{PageGeometry, PageRotation};

    fn slot(width_pt: f32, height_pt: f32) -> PageSlot {
        let picture = Picture::new();
        let overlay = Overlay::new();
        overlay.set_child(Some(&picture));
        PageSlot {
            overlay,
            picture,
            highlights: gtk::DrawingArea::new(),
            characters: None,
            characters_requested: false,
            content: None,
            width_pt,
            height_pt,
            rotation: PageRotation::None,
            state: PageState::Idle,
            target_dpi: 72,
            budget: crate::app::layout::TileBudget {
                factor: 1.0,
                base_dpi: 72,
            },
            tiles: Default::default(),
            tile_dpi: 0,
            tile_generation: 0,
            tile_failed_dpi: 0,
        }
    }

    fn geometry(width_pt: f32, height_pt: f32) -> PageGeometry {
        PageGeometry {
            width_pt,
            height_pt,
            rotation: PageRotation::None,
        }
    }

    #[gtk::test]
    fn gtk_ui_the_slots_are_kept_only_when_every_page_keeps_its_size() {
        let pages = [slot(595.0, 842.0), slot(842.0, 595.0)];

        assert!(same_layout(
            &pages,
            &[geometry(595.0, 842.0), geometry(842.0, 595.0)]
        ));
        assert!(
            !same_layout(&pages, &[geometry(595.0, 842.0)]),
            "a page was removed"
        );
        assert!(
            !same_layout(&pages, &[geometry(595.0, 842.0), geometry(595.0, 842.0)]),
            "a page changed size or was swapped for a different one"
        );
    }

    /// The swap's whole point: the bitmap stays up so the page does not flash
    /// blank, but everything read from the old handle is forgotten and the
    /// page is queued for a fresh render.
    #[gtk::test]
    fn gtk_ui_a_reset_slot_keeps_its_picture_and_forgets_the_old_handle() {
        let mut page = slot(595.0, 842.0);
        let pixbuf = gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, true, 8, 1, 1)
            .expect("a 1x1 pixbuf is always creatable");
        page.picture.set_pixbuf(Some(&pixbuf));
        page.state = PageState::Rendered;
        page.characters_requested = true;
        page.content = Some(PageContent::default());
        page.tile_dpi = 300;

        reset_slot(&mut page);

        assert!(page.state == PageState::Idle);
        assert!(page.picture.paintable().is_some());
        assert!(page.content.is_none());
        assert!(page.characters.is_none());
        assert!(!page.characters_requested);
        assert_eq!(page.tile_dpi, 0);
        assert_eq!(page.tile_generation, 1);
    }
}
