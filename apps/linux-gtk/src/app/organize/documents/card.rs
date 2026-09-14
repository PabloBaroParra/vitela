//! One block card: the stacked cover, the two lines of text, and the five
//! controls that move, turn or delete the whole block without a drag.

use gtk::prelude::*;
use gtk::{gdk, glib, Align, Box as GtkBox, Button, DragSource, Label, Orientation, Picture};
use pdf_render::DocumentHandle;

use crate::app::icons::{build_icon, Icon, ACCENT_TINT};
use crate::app::state::Viewer;

use super::super::command::{delete_block, rotate_block};
use super::super::grid::thumbnail::spawn_thumbnail;
use super::gap::drop_block;
use super::{rebuilt, rows, Row};

/// A quarter-turn, in the sign `Command::RotatePages` reads — negative
/// anticlockwise, positive clockwise. The same gesture the Pages view's
/// footer records for one page (`grid::card::QUARTER_TURN_DEGREES`), kept as
/// its own constant rather than shared: the two views hold their own card
/// vocabulary, and a block turn is not "the page turn, applied N times".
const QUARTER_TURN_DEGREES: i32 = 90;

/// The block cover. Smaller than the Pages view's card (140x180): a block
/// card is a row with room for a name, a page count and its controls, not a
/// thumbnail with a caption.
pub(in crate::app::organize) const COVER_WIDTH_PX: i32 = 96;
pub(in crate::app::organize) const COVER_HEIGHT_PX: i32 = 124;

/// How far each sheet behind the cover peeks out, in px. Two of them, so a
/// block reads as "a document" at a glance without a badge saying so.
const STACK_OFFSET_PX: i32 = 4;

/// One block card: the stacked cover, the name and range, and the five
/// controls that do without a mouse what the drag and the gaps do with one —
/// plus the two turns, which have no drag gesture at all.
pub(super) fn build_card(
    viewer: &Viewer,
    position: usize,
    total: usize,
    row: &Row,
    handle: DocumentHandle,
) -> GtkBox {
    let card = GtkBox::new(Orientation::Horizontal, 12);
    card.add_css_class("organize-block");
    card.set_focusable(true);
    card.update_property(&[gtk::accessible::Property::Label(&format!(
        "{}, {}, document {} of {total}",
        row.title(),
        row.meta(),
        position + 1
    ))]);

    card.append(&build_cover(viewer, row, handle));

    let details = GtkBox::new(Orientation::Vertical, 4);
    details.set_hexpand(true);
    details.set_valign(Align::Center);
    let name = Label::new(Some(&row.title()));
    name.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    name.add_css_class("organize-block-name");
    details.append(&name);
    let meta = Label::new(Some(&row.meta()));
    meta.set_xalign(0.0);
    meta.add_css_class("organize-block-meta");
    details.append(&meta);
    card.append(&details);

    let controls = GtkBox::new(Orientation::Horizontal, 4);
    controls.set_valign(Align::Center);
    // Keyboard parity with the drag, not a fallback for it: moving up is
    // "into the slot before the previous block", moving down is "into the
    // slot after the next one" — the same slots the gaps offer.
    controls.append(&move_button(
        viewer,
        row,
        Icon::MoveUp,
        "Move up",
        position.checked_sub(1),
    ));
    controls.append(&move_button(
        viewer,
        row,
        Icon::MoveDown,
        "Move down",
        (position + 2 <= total).then_some(position + 2),
    ));
    // The two turns sit between the moves and the delete, so the destructive
    // control keeps the end of the row — the same order the Pages view's
    // footer settled on, for the same reason.
    controls.append(&rotate_button(
        viewer,
        row,
        Icon::RotateLeft,
        "Rotate left",
        -QUARTER_TURN_DEGREES,
    ));
    controls.append(&rotate_button(
        viewer,
        row,
        Icon::RotateRight,
        "Rotate right",
        QUARTER_TURN_DEGREES,
    ));
    controls.append(&delete_button(viewer, row));
    card.append(&controls);

    let drag_source = DragSource::new();
    drag_source.set_actions(gdk::DragAction::MOVE);
    drag_source.connect_prepare({
        let anchor = row.anchor;
        move |_, _, _| {
            Some(gdk::ContentProvider::for_value(&glib::Value::from(
                anchor.0,
            )))
        }
    });
    card.add_controller(drag_source);

    card
}

/// The cover: two decorative sheets behind one real thumbnail of the block's
/// first page.
fn build_cover(viewer: &Viewer, row: &Row, handle: DocumentHandle) -> GtkBox {
    let cover = GtkBox::new(Orientation::Horizontal, 0);
    cover.set_valign(Align::Center);

    let stack = gtk::Overlay::new();
    stack.set_size_request(
        COVER_WIDTH_PX + 2 * STACK_OFFSET_PX,
        COVER_HEIGHT_PX + 2 * STACK_OFFSET_PX,
    );
    for depth in [2, 1] {
        let sheet = GtkBox::new(Orientation::Horizontal, 0);
        sheet.add_css_class("organize-cover-sheet");
        sheet.set_size_request(COVER_WIDTH_PX, COVER_HEIGHT_PX);
        sheet.set_halign(Align::Start);
        sheet.set_valign(Align::Start);
        sheet.set_margin_start(depth * STACK_OFFSET_PX);
        sheet.set_margin_top(depth * STACK_OFFSET_PX);
        // A single-page block is one sheet, and drawing a stack behind it
        // would claim pages it does not have.
        sheet.set_visible(row.count > depth as usize);
        stack.add_overlay(&sheet);
    }

    let picture = Picture::new();
    picture.add_css_class("organize-cover");
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_size_request(COVER_WIDTH_PX, COVER_HEIGHT_PX);
    picture.set_halign(Align::Start);
    picture.set_valign(Align::Start);
    stack.add_overlay(&picture);
    cover.append(&stack);

    if let Some(cover_index) = row.cover {
        spawn_thumbnail(
            viewer,
            handle,
            row.anchor,
            cover_index as u32,
            picture,
            (COVER_WIDTH_PX, COVER_HEIGHT_PX),
        );
    }
    cover
}

/// A Move up/Move down button for `row`, insensitive when `slot` is `None` —
/// the first block cannot move up, the last cannot move down.
fn move_button(viewer: &Viewer, row: &Row, icon: Icon, label: &str, slot: Option<usize>) -> Button {
    let button = Button::new();
    button.set_child(Some(&build_icon(icon, 16, ACCENT_TINT)));
    button.add_css_class("flat");
    let name = format!("{label}: {}", row.title());
    button.update_property(&[gtk::accessible::Property::Label(&name)]);
    button.set_tooltip_text(Some(&name));
    button.set_sensitive(slot.is_some());
    if let Some(slot) = slot {
        button.connect_clicked({
            let viewer = viewer.clone();
            let anchor = row.anchor;
            move |_| {
                drop_block(&viewer, anchor, slot);
            }
        });
    }
    button
}

/// One of the block's two quarter-turn buttons: every page of the block
/// turns `delta_degrees`, as one undoable step.
///
/// Deliberately does **not** call [`rebuilt`] the way [`delete_button`] does,
/// and that is the one thing to get right here. A rotation changes no block's
/// extent, so there is nothing about the list to rebuild — and repopulating
/// now would be actively wrong: the cover would re-render against the pdfium
/// handle as it still is, which holds the *pre-rotation* bytes, and cache the
/// old angle back under a key [`rotate_block`] has just emptied. The rebuild
/// that shows the turn is the one `organize::refresh_after_reopen` runs once
/// the reopen behind the command lands.
fn rotate_button(
    viewer: &Viewer,
    row: &Row,
    icon: Icon,
    label: &str,
    delta_degrees: i32,
) -> Button {
    let button = Button::new();
    button.set_child(Some(&build_icon(icon, 16, ACCENT_TINT)));
    button.add_css_class("flat");
    let name = format!("{label}: {}", row.title());
    button.update_property(&[gtk::accessible::Property::Label(&name)]);
    button.set_tooltip_text(Some(&name));
    button.connect_clicked({
        let viewer = viewer.clone();
        let anchor = row.anchor;
        // Resolved again on click, never captured, for the reason
        // [`delete_button`] gives: the row this card was built from is a
        // snapshot, and the screen's own header can have moved the block
        // since.
        move |_| {
            let Some((rows, _)) = rows(&viewer) else {
                return;
            };
            if let Some(row) = rows.iter().find(|row| row.anchor == anchor) {
                rotate_block(&viewer, row.start, row.count, delta_degrees);
            }
        }
    });
    button
}

/// The block's delete button. Confirmation is the undo stack, the same
/// bargain the per-page delete strikes: one `Command::RemovePages` step puts
/// the whole block back.
fn delete_button(viewer: &Viewer, row: &Row) -> Button {
    let button = Button::new();
    button.set_child(Some(&build_icon(Icon::Delete, 16, ACCENT_TINT)));
    button.add_css_class("flat");
    let name = format!("Delete {}", row.title());
    button.update_property(&[gtk::accessible::Property::Label(&name)]);
    button.set_tooltip_text(Some(&name));
    button.connect_clicked({
        let viewer = viewer.clone();
        let anchor = row.anchor;
        // Resolved again on click rather than captured: the card outlives
        // nothing here, but the *row* it was built from is a snapshot, and
        // an undo from the screen's own header can have moved the block
        // since.
        move |_| {
            let Some((rows, _)) = rows(&viewer) else {
                return;
            };
            if let Some(row) = rows.iter().find(|row| row.anchor == anchor) {
                rebuilt(&viewer, delete_block(&viewer, row.start, row.count));
            }
        }
    });
    button
}
