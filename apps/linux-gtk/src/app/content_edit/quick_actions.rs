//! The floating quick-action bar beside whatever is selected in content-edit
//! mode — Acrobat's "Edit PDF" bubble, offering only what this shell can
//! already do: retype a text block, replace an image, cut, copy and delete.
//!
//! Every button calls the very handler its existing door calls — the F2
//! action, the Edit card's Replace button, CTRL+X, CTRL+C, the Delete key —
//! and is sensitive under the same gate that door is. Nothing here decides
//! anything about an edit; it only shows the doors in one more place.
//!
//! The bar is a child of the canvas's own `Overlay` (`page_area`), placed by
//! its margins, rather than a `GtkPopover`: an overlay child is clipped to
//! the canvas and never takes a grab or the focus, and where it goes is this
//! module's arithmetic ([`bar_origin`]) rather than a popover's own idea of
//! "on screen", which is the window, not the canvas.
//!
//! It follows the selection through scrolling and zoom by re-placing itself
//! on an idle after any of them ([`sync`]), when the canvas has been laid
//! out again; it hides while a press is being dragged, while an inline editor
//! is open, when the selection clears and when the mode goes off.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{glib, graphene, Align, Box as GtkBox, Button, Orientation};
use pdf_document::Rect;
use pdf_render::{place_rect, PlacedRect, TextRect};

use crate::app::i18n::tr;
use crate::app::icons::Icon;
use crate::app::state::{DocumentSession, Viewer, ViewerState};

use super::{block, clipboard, hover, image, image_controls, multi, panel};

/// The gap between the selection's box and the bar, in device pixels.
const GAP_PX: f64 = 6.0;

/// The bar's own look, appended to the shell's stylesheet beside the Edit
/// page's. The buttons are the Edit page's tiles, made compact.
pub(crate) const QUICK_ACTIONS_CSS: &str = r#"
.content-quick-actions {
  background: @vitela_surface;
  border: 1px solid @vitela_border;
  border-radius: 10px;
  padding: 3px;
}

.content-quick-actions .edit-tile {
  padding: 3px 8px;
  border-radius: 7px;
}
"#;

/// The bar and its buttons. Held on the `Viewer`, which is why it is
/// `Clone`: every field is a widget handle, and `scheduled` is shared.
#[derive(Clone)]
pub(crate) struct QuickActions {
    pub(crate) root: GtkBox,
    /// Retypes the selected text block — the F2 action.
    pub(crate) retype: Button,
    /// Swaps the selected image for a file — the Edit card's Replace.
    pub(crate) replace: Button,
    pub(crate) cut: Button,
    pub(crate) copy: Button,
    /// Whatever the Delete key would delete.
    pub(crate) delete: Button,
    /// Whether a re-placement is already waiting on an idle, so a burst of
    /// redraws during one frame costs one.
    scheduled: Rc<Cell<bool>>,
}

/// Builds the bar, hidden. `build_ui` puts it on the canvas overlay and
/// [`connect`] wires it once the `Viewer` exists.
pub(crate) fn build() -> QuickActions {
    let root = GtkBox::new(Orientation::Horizontal, 2);
    root.add_css_class("content-quick-actions");
    root.set_halign(Align::Start);
    root.set_valign(Align::Start);
    root.set_visible(false);
    root.update_property(&[gtk::accessible::Property::Label(&tr("edit-quick-actions"))]);

    let retype = Button::new();
    panel::tile(
        &retype,
        &tr("edit-quick-retype"),
        Icon::Text,
        &tr("edit-quick-retype-tooltip"),
    );
    let replace = Button::new();
    panel::tile(
        &replace,
        &tr("edit-quick-replace"),
        Icon::Image,
        &tr("edit-replace-image-tooltip"),
    );
    // No icon of the shell's own says "cut" or "copy", and the icon theme
    // is off limits (`shell`'s module doc), so these two are words alone.
    let cut = text_tile(&tr("edit-quick-cut"), &tr("edit-quick-cut-tooltip"));
    let copy = text_tile(&tr("edit-quick-copy"), &tr("edit-quick-copy-tooltip"));
    let delete = Button::new();
    panel::tile(
        &delete,
        &tr("edit-quick-delete"),
        Icon::Delete,
        &tr("edit-quick-delete-tooltip"),
    );
    for button in [&retype, &replace, &cut, &copy, &delete] {
        // A click on the bar must leave the keyboard where it was: Delete,
        // F2 and the clipboard keys stand down while a text field has it.
        button.set_focus_on_click(false);
        root.append(button);
    }

    QuickActions {
        root,
        retype,
        replace,
        cut,
        copy,
        delete,
        scheduled: Rc::new(Cell::new(false)),
    }
}

fn text_tile(label: &str, tooltip: &str) -> Button {
    let button = Button::with_label(label);
    button.add_css_class("edit-tile");
    button.set_tooltip_text(Some(tooltip));
    button
}

/// Wires every button to its existing handler, and re-places the bar
/// whenever the canvas scrolls or changes size.
pub(crate) fn connect(viewer: &Viewer) {
    let actions = &viewer.content_quick_actions;
    actions.retype.connect_clicked({
        let viewer = viewer.clone();
        move |_| {
            block::retype_selected(&viewer);
        }
    });
    actions.replace.connect_clicked({
        let viewer = viewer.clone();
        move |_| match viewer.window() {
            Some(window) => image::replace_selected(&window, &viewer),
            None => viewer.status.set_text(&tr("edit-no-window")),
        }
    });
    actions.cut.connect_clicked({
        let viewer = viewer.clone();
        move |_| {
            clipboard::try_cut(&viewer);
        }
    });
    actions.copy.connect_clicked({
        let viewer = viewer.clone();
        move |_| {
            clipboard::try_copy(&viewer);
        }
    });
    actions.delete.connect_clicked({
        let viewer = viewer.clone();
        move |_| {
            super::delete_selection(&viewer);
        }
    });
    for adjustment in [viewer.scroll.vadjustment(), viewer.scroll.hadjustment()] {
        adjustment.connect_value_changed({
            let viewer = viewer.clone();
            move |_| sync(&viewer)
        });
        adjustment.connect_changed({
            let viewer = viewer.clone();
            move |_| sync(&viewer)
        });
    }
}

/// What the bar offers for the selection as it stands, and where that
/// selection is — `None` when there should be no bar at all.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Target {
    pub(crate) page_index: usize,
    /// The selection's box, in PDF page space.
    pub(crate) bbox: Rect,
    /// `Some` only for a single text block, with whether it may be retyped.
    pub(crate) retype: Option<bool>,
    /// `Some` only for a single image, with whether it may be replaced.
    pub(crate) replace: Option<bool>,
    pub(crate) cut: bool,
    pub(crate) copy: bool,
    pub(crate) delete: bool,
}

/// The bar for `state`, read from the same gates every existing door reads:
/// the image card's [`image_controls`] for an image, the document's
/// content-edit permission for everything that changes the page, and its
/// extraction permission for everything that copies it out.
pub(crate) fn target(state: &ViewerState) -> Option<Target> {
    if !state.content_edit_mode || state.content_insert_mode.is_some() {
        return None;
    }
    let session = state.session.as_ref()?;
    if hover::dragging(session) || session.content_editor.is_some() {
        return None;
    }
    let editable = session.content_edit_access.refusal().is_none();
    let copy = session.text_access.refusal().is_none();
    let (page_index, bbox, retype, replace, delete) = selected(session, editable)?;
    Some(Target {
        page_index,
        bbox,
        retype,
        replace,
        cut: copy && editable,
        copy,
        delete,
    })
}

/// The selection's page, box, retype and replace offers and deletability —
/// in the order the Delete key asks: several items, a block, an image, a
/// shape. At most one of them is selected at a time.
#[allow(clippy::type_complexity)]
fn selected(
    session: &DocumentSession,
    editable: bool,
) -> Option<(usize, Rect, Option<bool>, Option<bool>, bool)> {
    if let Some(held) = session.content_selection.as_ref() {
        return Some((
            held.page_index,
            multi::union(&held.members)?,
            None,
            None,
            editable,
        ));
    }
    if let Some(block) = session.selected_text_block.as_ref() {
        return Some((
            block.page_index,
            block.run.bbox,
            Some(editable),
            None,
            editable,
        ));
    }
    if let Some(selected) = session.selected_image.as_ref() {
        let controls = image_controls(Some(session));
        return Some((
            selected.page_index,
            selected.item.bbox,
            None,
            Some(controls.replace_enabled()),
            controls.delete_enabled(),
        ));
    }
    session.selected_path.as_ref().map(|selected| {
        (
            selected.page_index,
            selected.item.bbox,
            None,
            None,
            editable,
        )
    })
}

/// Where the bar of `size` goes beside `anchor` (the selection's box) on a
/// canvas of `canvas` size, all in canvas pixels — or `None` when the
/// selection is scrolled entirely out of view.
///
/// To the right of the selection's top-right corner, top edges level; to
/// the left of the selection when that would run off the canvas; above it —
/// or below, when there is no room above — when neither side has room. Then
/// clamped onto the canvas, so a selection running off an edge keeps its bar
/// on screen.
pub(crate) fn bar_origin(
    anchor: PlacedRect,
    size: (f64, f64),
    canvas: (f64, f64),
) -> Option<(f64, f64)> {
    let (width, height) = size;
    let (canvas_width, canvas_height) = canvas;
    let right = anchor.left + anchor.width;
    let bottom = anchor.top + anchor.height;
    let visible =
        right >= 0.0 && anchor.left <= canvas_width && bottom >= 0.0 && anchor.top <= canvas_height;
    if !visible {
        return None;
    }

    let (mut x, mut y) = (right + GAP_PX, anchor.top);
    if x + width > canvas_width {
        x = anchor.left - GAP_PX - width;
        if x < 0.0 {
            x = right - width;
            y = anchor.top - GAP_PX - height;
            if y < 0.0 {
                y = bottom + GAP_PX;
            }
        }
    }
    let x = x.min(canvas_width - width).max(0.0);
    let y = y.min(canvas_height - height).max(0.0);
    Some((x, y))
}

/// Re-places the bar once the canvas is laid out, coalescing every request
/// made before that idle runs into one.
pub(crate) fn sync(viewer: &Viewer) {
    let actions = &viewer.content_quick_actions;
    if actions.scheduled.replace(true) {
        return;
    }
    let viewer = viewer.clone();
    glib::idle_add_local_once(move || {
        viewer.content_quick_actions.scheduled.set(false);
        refresh(&viewer);
    });
}

/// Shows the bar for the selection as it stands, where it belongs, or hides
/// it.
pub(crate) fn refresh(viewer: &Viewer) {
    let actions = &viewer.content_quick_actions;
    let Some(target) = target(&viewer.state.borrow()) else {
        actions.root.set_visible(false);
        return;
    };
    apply(actions, &target);
    // `measure` answers 0 for a hidden widget and counts its margins, so
    // the bar is shown and unmargined before `origin` sizes it. All of this
    // happens before the next frame is laid out: nothing flashes.
    actions.root.set_visible(true);
    actions.root.set_margin_start(0);
    actions.root.set_margin_top(0);
    match origin(viewer, &target) {
        Some((x, y)) => {
            actions.root.set_margin_start(x.round() as i32);
            actions.root.set_margin_top(y.round() as i32);
        }
        None => actions.root.set_visible(false),
    }
}

/// Spends `target` on the buttons: which are shown, and which may be
/// pressed.
pub(crate) fn apply(actions: &QuickActions, target: &Target) {
    actions.retype.set_visible(target.retype.is_some());
    actions.retype.set_sensitive(target.retype == Some(true));
    actions.replace.set_visible(target.replace.is_some());
    actions.replace.set_sensitive(target.replace == Some(true));
    actions.cut.set_sensitive(target.cut);
    actions.copy.set_sensitive(target.copy);
    actions.delete.set_sensitive(target.delete);
}

/// The bar's origin on the canvas for `target`, measured off the page's
/// layer as it is laid out now.
fn origin(viewer: &Viewer, target: &Target) -> Option<(f64, f64)> {
    let actions = &viewer.content_quick_actions;
    let canvas = actions.root.parent()?;
    let (layer, placed) = {
        let state = viewer.state.borrow();
        let page = state.session.as_ref()?.pages.get(target.page_index)?;
        let placed = place_rect(
            TextRect {
                x_pt: target.bbox.x as f32,
                y_pt: target.bbox.y as f32,
                width_pt: target.bbox.width as f32,
                height_pt: target.bbox.height as f32,
            },
            page.placement(),
        );
        (page.highlights.clone(), placed)
    };
    let corner = layer.compute_point(
        &canvas,
        &graphene::Point::new(placed.left as f32, placed.top as f32),
    )?;
    let anchor = PlacedRect {
        left: f64::from(corner.x()),
        top: f64::from(corner.y()),
        ..placed
    };
    let (_, width, _, _) = actions.root.measure(Orientation::Horizontal, -1);
    let (_, height, _, _) = actions.root.measure(Orientation::Vertical, width);
    bar_origin(
        anchor,
        (f64::from(width), f64::from(height)),
        (f64::from(canvas.width()), f64::from(canvas.height())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::content_edit::test_support::Fixture;
    use pdf_render::PageRotation;

    fn rect(left: f64, top: f64, width: f64, height: f64) -> PlacedRect {
        PlacedRect {
            left,
            top,
            width,
            height,
        }
    }

    const BAR: (f64, f64) = (200.0, 30.0);
    const CANVAS: (f64, f64) = (800.0, 600.0);

    #[test]
    fn the_bar_sits_right_of_the_top_right_corner() {
        let anchor = rect(100.0, 100.0, 150.0, 40.0);

        assert_eq!(
            bar_origin(anchor, BAR, CANVAS),
            Some((250.0 + GAP_PX, 100.0))
        );
    }

    #[test]
    fn the_bar_flips_left_when_the_right_has_no_room() {
        let anchor = rect(500.0, 100.0, 250.0, 40.0);

        assert_eq!(
            bar_origin(anchor, BAR, CANVAS),
            Some((500.0 - GAP_PX - 200.0, 100.0))
        );
    }

    #[test]
    fn the_bar_goes_above_when_neither_side_has_room() {
        let anchor = rect(100.0, 300.0, 650.0, 40.0);

        assert_eq!(
            bar_origin(anchor, BAR, CANVAS),
            Some((750.0 - 200.0, 300.0 - GAP_PX - 30.0))
        );
    }

    #[test]
    fn the_bar_goes_below_when_there_is_no_room_above_either() {
        let anchor = rect(100.0, 10.0, 650.0, 40.0);

        assert_eq!(
            bar_origin(anchor, BAR, CANVAS),
            Some((550.0, 50.0 + GAP_PX))
        );
    }

    #[test]
    fn a_selection_off_an_edge_keeps_its_bar_on_the_canvas() {
        // Running off the bottom: the bar is pulled back up.
        let low = rect(100.0, 590.0, 100.0, 40.0);
        assert_eq!(bar_origin(low, BAR, CANVAS), Some((206.0, 570.0)));
        // A canvas narrower than the bar has no side for it, nor room above:
        // it goes below, pinned to the left edge.
        let anchor = rect(10.0, 10.0, 20.0, 20.0);
        assert_eq!(bar_origin(anchor, BAR, (150.0, 600.0)), Some((0.0, 36.0)));
    }

    #[test]
    fn a_selection_scrolled_out_of_view_has_no_bar() {
        assert_eq!(
            bar_origin(rect(100.0, -80.0, 50.0, 40.0), BAR, CANVAS),
            None
        );
        assert_eq!(
            bar_origin(rect(100.0, 700.0, 50.0, 40.0), BAR, CANVAS),
            None
        );
        assert_eq!(
            bar_origin(rect(-300.0, 10.0, 50.0, 40.0), BAR, CANVAS),
            None
        );
    }

    fn select_first_block(fixture: &Fixture) {
        let run = &fixture.runs[0];
        super::super::resolve_click(fixture.viewer(), 0, Fixture::middle(run));
    }

    fn current(fixture: &Fixture) -> Option<Target> {
        target(&fixture.viewer().state.borrow())
    }

    /// Nothing selected, no bar; a selected block offers retype, cut, copy
    /// and delete, and no replace.
    #[gtk::test]
    fn gtk_ui_a_selected_block_offers_its_text_actions() {
        let fixture = Fixture::new(PageRotation::None);
        assert_eq!(current(&fixture), None);

        select_first_block(&fixture);

        let target = current(&fixture).expect("a selected block has a bar");
        assert_eq!(target.page_index, 0);
        assert_eq!(target.retype, Some(true));
        assert_eq!(target.replace, None);
        assert!(target.cut && target.copy && target.delete);
        let actions = &fixture.viewer().content_quick_actions;
        apply(actions, &target);
        assert!(actions.retype.get_visible() && actions.retype.is_sensitive());
        assert!(!actions.replace.get_visible());
        assert!(actions.delete.is_sensitive());
    }

    /// A drag in flight, an open editor and the mode going off each take the
    /// bar away.
    #[gtk::test]
    fn gtk_ui_the_bar_stands_down_while_dragging_editing_or_disarmed() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        select_first_block(&fixture);
        assert!(current(&fixture).is_some());

        let press = Fixture::middle(&fixture.runs[0]);
        assert!(super::super::text::begin_text_drag(viewer, 0, press));
        assert_eq!(current(&fixture), None, "hidden while dragging");
        viewer
            .state
            .borrow_mut()
            .session
            .as_mut()
            .expect("a session")
            .text_drag = None;
        assert!(current(&fixture).is_some());

        block::retype_selected(viewer);
        assert_eq!(current(&fixture), None, "hidden while retyping");
        super::super::editor::cancel(viewer);

        select_first_block(&fixture);
        assert!(current(&fixture).is_some());
        super::super::set_mode(viewer, false);
        assert_eq!(current(&fixture), None, "hidden with the mode off");
    }

    /// A document that refuses content changes keeps Copy and loses
    /// everything that would change the page — the same answer the Edit
    /// card and the keys give.
    #[gtk::test]
    fn gtk_ui_a_locked_document_keeps_copy_and_loses_the_rest() {
        let fixture = Fixture::new(PageRotation::None);
        select_first_block(&fixture);
        fixture
            .viewer()
            .state
            .borrow_mut()
            .session
            .as_mut()
            .expect("a session")
            .content_edit_access = crate::app::state::ContentEditAccess::Forbidden;

        let target = current(&fixture).expect("the selection still has a bar");

        assert!(target.copy);
        assert!(!target.cut && !target.delete);
        assert_eq!(target.retype, Some(false));
        let actions = &fixture.viewer().content_quick_actions;
        apply(actions, &target);
        assert!(actions.copy.is_sensitive());
        assert!(!actions.cut.is_sensitive());
        assert!(!actions.delete.is_sensitive());
    }

    /// Runs every idle the bar has queued.
    fn settle() {
        let context = glib::MainContext::default();
        while context.iteration(false) {}
    }

    /// Where the page's layer starts on the canvas the bar is placed on.
    fn layer_origin(fixture: &Fixture) -> (f64, f64) {
        let viewer = fixture.viewer();
        let canvas = viewer
            .content_quick_actions
            .root
            .parent()
            .expect("the bar is on the canvas overlay");
        let state = viewer.state.borrow();
        let layer = &state.session.as_ref().expect("a session").pages[0].highlights;
        let origin = layer
            .compute_point(&canvas, &graphene::Point::new(0.0, 0.0))
            .expect("the layer is on the canvas");
        (f64::from(origin.x()), f64::from(origin.y()))
    }

    /// The scheduled refresh puts the bar beside the selection's top-right
    /// corner as the page draws it, and takes it away again once an inline
    /// editor opens over the selection.
    #[gtk::test]
    fn gtk_ui_the_bar_shows_beside_a_selection_and_hides_for_the_editor() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        let root = &viewer.content_quick_actions.root;
        settle();
        assert!(!root.get_visible(), "no selection, no bar");

        select_first_block(&fixture);
        settle();
        assert_bar_beside_first_block(&fixture);

        block::retype_selected(viewer);
        settle();
        assert!(!root.get_visible(), "hidden under the editor");
        super::super::editor::cancel(viewer);
    }

    /// Asserts the bar is shown where [`bar_origin`] puts it beside the
    /// first block's box as the page's placement draws it.
    fn assert_bar_beside_first_block(fixture: &Fixture) {
        let root = &fixture.viewer().content_quick_actions.root;
        assert!(root.get_visible());
        let bbox = fixture.runs[0].bbox;
        let placed = {
            let state = fixture.viewer().state.borrow();
            let page = &state.session.as_ref().expect("a session").pages[0];
            let bbox = TextRect {
                x_pt: bbox.x as f32,
                y_pt: bbox.y as f32,
                width_pt: bbox.width as f32,
                height_pt: bbox.height as f32,
            };
            place_rect(bbox, page.placement())
        };
        let (layer_x, layer_y) = layer_origin(fixture);
        let anchor = rect(
            layer_x + placed.left,
            layer_y + placed.top,
            placed.width,
            placed.height,
        );
        let canvas = root.parent().expect("on the canvas");
        let canvas = (f64::from(canvas.width()), f64::from(canvas.height()));
        let (x, y) = (root.margin_start(), root.margin_top());
        root.set_margin_start(0);
        root.set_margin_top(0);
        let (_, width, _, _) = root.measure(Orientation::Horizontal, -1);
        let (_, height, _, _) = root.measure(Orientation::Vertical, width);
        assert!(width > 0, "the bar was measured while shown");
        let expected = bar_origin(anchor, (f64::from(width), f64::from(height)), canvas)
            .expect("the block is in view");
        assert_eq!(
            (x, y),
            (expected.0.round() as i32, expected.1.round() as i32)
        );
        assert!(
            f64::from(x + width) <= canvas.0,
            "the bar fits on the canvas"
        );
    }

    /// On a page turned a quarter the same block is drawn where the turn
    /// puts it — here past the canvas's right edge, which an unturned page
    /// would not do — so the bar, placed through the page's placement,
    /// stays away rather than floating beside the unturned spot.
    #[gtk::test]
    fn gtk_ui_the_bar_follows_the_page_rotation() {
        let fixture = Fixture::new(PageRotation::Clockwise90);
        let root = &fixture.viewer().content_quick_actions.root;
        select_first_block(&fixture);
        settle();

        let bbox = fixture.runs[0].bbox;
        let (layer_x, _) = layer_origin(&fixture);
        let left = [
            (bbox.x, bbox.y),
            (bbox.x + bbox.width, bbox.y),
            (bbox.x, bbox.y + bbox.height),
            (bbox.x + bbox.width, bbox.y + bbox.height),
        ]
        .into_iter()
        .map(|corner| fixture.on_screen(corner).0)
        .fold(f64::INFINITY, f64::min);
        let canvas_width = f64::from(root.parent().expect("on the canvas").width());
        assert!(
            layer_x + left > canvas_width,
            "the fixture's turned block is off the canvas"
        );
        assert!(!root.get_visible());
    }

    /// Turned the other way, the same block is drawn near the canvas's left
    /// edge, in view: the bar shows beside it as the turned page draws it.
    #[gtk::test]
    fn gtk_ui_the_bar_shows_beside_a_block_on_a_turned_page() {
        let fixture = Fixture::new(PageRotation::Clockwise270);
        select_first_block(&fixture);
        settle();

        assert_bar_beside_first_block(&fixture);
    }

    /// The buttons call the doors they stand for: Delete removes the
    /// selected block as one queued command, as the key does.
    #[gtk::test]
    fn gtk_ui_the_delete_button_deletes_what_the_key_deletes() {
        let fixture = Fixture::new(PageRotation::None);
        let viewer = fixture.viewer();
        select_first_block(&fixture);
        // Held back so the delete queues its refresh instead of reopening
        // anything through pdfium in a test.
        viewer.state.borrow_mut().preview_refresh_in_flight = true;
        apply(
            &viewer.content_quick_actions,
            &current(&fixture).expect("a bar"),
        );

        viewer.content_quick_actions.delete.emit_clicked();

        let state = viewer.state.borrow();
        let session = state.session.as_ref().expect("a session");
        assert!(session.selected_text_block.is_none());
        assert_eq!(
            session
                .document_model
                .as_ref()
                .expect("a model")
                .pending_edits
                .entries()
                .len(),
            1
        );
    }
}
