//! The pad dialog: a white drawing area the user signs on with the mouse (or a
//! pen), **Use** that stays disabled until there is a line, **Clear**,
//! **Cancel**, and a **Remember on this computer** check that is ticked by
//! default.
//!
//! The pad's rules live in [`ink::Pad`]; this file is the widgets around it and
//! the paint. The pad is white whatever the theme — ink is always black, as on
//! paper — and is drawn that way rather than styled, so a dark theme cannot
//! turn the signature into black on near-black.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    cairo, Box as GtkBox, Button, CheckButton, DrawingArea, GestureDrag, Orientation, Window,
};

use crate::app::state::Viewer;

use super::flow;
use super::ink::{self, Pad, Point};

const TITLE: &str = "Draw your signature";
const REMEMBER_LABEL: &str = "Remember on this computer";
const PAD_WIDTH: i32 = 480;
const PAD_HEIGHT: i32 = 200;
const MARGIN: i32 = 12;
const BORDER_GREY: f64 = 0.6;

/// The pad dialog and the handles that drive it. Cheap to clone: every field is
/// a reference-counted widget or a shared pad.
#[derive(Clone)]
pub(super) struct PadView {
    pub(super) window: Window,
    pub(super) clear: Button,
    pub(super) cancel: Button,
    pub(super) use_button: Button,
    pub(super) remember: CheckButton,
    area: DrawingArea,
    pad: Rc<RefCell<Pad>>,
}

impl PadView {
    /// The pen came down at `point`, in pad pixels.
    pub(super) fn press(&self, point: Point) {
        self.pad.borrow_mut().begin(point);
        self.refresh();
    }

    /// The pen moved to `point`.
    pub(super) fn drag_to(&self, point: Point) {
        self.pad.borrow_mut().extend(point);
        self.refresh();
    }

    /// The pen came up.
    pub(super) fn release(&self) {
        self.pad.borrow_mut().end();
        self.refresh();
    }

    /// Wipes the pad.
    fn wipe(&self) {
        self.pad.borrow_mut().clear();
        self.refresh();
    }

    /// Brings the buttons and the paint in line with what is on the pad.
    fn refresh(&self) {
        {
            let pad = self.pad.borrow();
            self.use_button.set_sensitive(pad.has_ink());
            self.clear.set_sensitive(!pad.is_blank());
        }
        self.area.queue_draw();
    }
}

/// Opens the pad over the window, for a signature that will be placed on
/// `session_id`'s document.
pub(super) fn open(viewer: &Viewer, session_id: u64) -> PadView {
    let pad = Rc::new(RefCell::new(Pad::default()));

    let area = DrawingArea::new();
    area.set_content_width(PAD_WIDTH);
    area.set_content_height(PAD_HEIGHT);
    area.set_hexpand(true);
    area.set_draw_func({
        let pad = pad.clone();
        move |_, context, width, height| paint(&pad.borrow(), context, width, height)
    });

    let remember = CheckButton::with_label(REMEMBER_LABEL);
    remember.set_active(true);

    let clear = Button::with_label("Clear");
    let cancel = Button::with_label("Cancel");
    let use_button = Button::with_label("Use");
    use_button.add_css_class("suggested-action");
    // Nothing to use and nothing to clear until the pen has drawn a line.
    use_button.set_sensitive(false);
    clear.set_sensitive(false);

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.append(&clear);
    buttons.append(&cancel);
    buttons.append(&use_button);

    let content = GtkBox::new(Orientation::Vertical, 8);
    content.set_margin_top(MARGIN);
    content.set_margin_bottom(MARGIN);
    content.set_margin_start(MARGIN);
    content.set_margin_end(MARGIN);
    content.append(&area);
    content.append(&remember);
    content.append(&buttons);

    let mut dialog = Window::builder().modal(true).title(TITLE).child(&content);
    if let Some(window) = viewer.window() {
        dialog = dialog.transient_for(&window);
    }
    let window = dialog.build();
    flow::track(viewer, &window);

    let view = PadView {
        window,
        clear,
        cancel,
        use_button,
        remember,
        area,
        pad,
    };
    connect_pen(&view);
    view.clear.connect_clicked({
        let view = view.clone();
        move |_| view.wipe()
    });
    view.cancel.connect_clicked({
        let viewer = viewer.clone();
        let window = view.window.clone();
        move |_| flow::dismiss(&viewer, &window)
    });
    view.use_button.connect_clicked({
        let viewer = viewer.clone();
        let view = view.clone();
        move |_| {
            let strokes = view.pad.borrow().strokes().to_vec();
            flow::use_drawn(
                &viewer,
                &view.window,
                session_id,
                strokes,
                view.remember.is_active(),
            );
        }
    });
    view.window.present();
    view
}

/// Forwards the pointer to the pad. `GestureDrag` reports a pen the way it
/// reports a mouse — a primary-button drag — so one gesture serves both.
fn connect_pen(view: &PadView) {
    let drag = GestureDrag::new();
    drag.connect_drag_begin({
        let view = view.clone();
        move |_, x, y| view.press((x, y))
    });
    drag.connect_drag_update({
        let view = view.clone();
        move |gesture, offset_x, offset_y| {
            // `start_point` is in the widget's coordinates and the offsets are
            // relative to it, so the pen is at their sum.
            if let Some((start_x, start_y)) = gesture.start_point() {
                view.drag_to((start_x + offset_x, start_y + offset_y));
            }
        }
    });
    drag.connect_drag_end({
        let view = view.clone();
        move |_, _, _| view.release()
    });
    view.area.add_controller(drag);
}

/// Paints the white pad, its border, and the ink — the finished strokes and the
/// one being drawn — with the same pen the PNG is made with.
fn paint(pad: &Pad, context: &cairo::Context, width: i32, height: i32) {
    context.set_source_rgb(1.0, 1.0, 1.0);
    let _ = context.paint();

    context.set_source_rgb(BORDER_GREY, BORDER_GREY, BORDER_GREY);
    context.set_line_width(1.0);
    context.rectangle(0.5, 0.5, f64::from(width) - 1.0, f64::from(height) - 1.0);
    let _ = context.stroke();

    context.set_source_rgb(0.0, 0.0, 0.0);
    context.set_line_width(ink::STROKE_WIDTH);
    context.set_line_cap(cairo::LineCap::Round);
    context.set_line_join(cairo::LineJoin::Round);
    let finished = pad.strokes().iter().map(Vec::as_slice);
    for stroke in finished.chain(std::iter::once(pad.current())) {
        let [first, rest @ ..] = stroke else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        context.move_to(first.0, first.1);
        for &(x, y) in rest {
            context.line_to(x, y);
        }
        let _ = context.stroke();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::test_support::{open_document, the_session, Built};

    fn a_pad(built: &Built) -> PadView {
        let viewer = built.viewer();
        open_document(viewer);
        open(viewer, the_session(viewer))
    }

    fn sign(view: &PadView) {
        view.press((10.0, 10.0));
        view.drag_to((60.0, 30.0));
        view.release();
    }

    #[gtk::test]
    fn gtk_ui_the_pad_opens_blank_with_nothing_to_use_or_clear() {
        let built = Built::new();
        let view = a_pad(&built);

        assert!(!view.use_button.is_sensitive());
        assert!(!view.clear.is_sensitive());
        assert_eq!(view.window.title().as_deref(), Some(TITLE));
        assert!(view.window.is_modal());
    }

    #[gtk::test]
    fn gtk_ui_a_drawn_line_enables_use_and_clear() {
        let built = Built::new();
        let view = a_pad(&built);

        sign(&view);

        assert!(view.use_button.is_sensitive());
        assert!(view.clear.is_sensitive());
    }

    #[gtk::test]
    fn gtk_ui_a_bare_click_leaves_use_disabled() {
        let built = Built::new();
        let view = a_pad(&built);

        view.press((10.0, 10.0));
        view.release();

        assert!(!view.use_button.is_sensitive());
    }

    #[gtk::test]
    fn gtk_ui_a_line_still_being_drawn_is_not_yet_usable() {
        let built = Built::new();
        let view = a_pad(&built);

        view.press((10.0, 10.0));
        view.drag_to((60.0, 30.0));

        assert!(!view.use_button.is_sensitive());
        assert!(view.clear.is_sensitive(), "there is something to wipe");
    }

    #[gtk::test]
    fn gtk_ui_clear_wipes_the_pad_and_disables_both_buttons_again() {
        let built = Built::new();
        let view = a_pad(&built);
        sign(&view);

        view.clear.emit_clicked();

        assert!(!view.use_button.is_sensitive());
        assert!(!view.clear.is_sensitive());
    }

    #[gtk::test]
    fn gtk_ui_remember_is_ticked_by_default() {
        let built = Built::new();
        let view = a_pad(&built);

        assert!(view.remember.is_active());
        assert_eq!(view.remember.label().as_deref(), Some(REMEMBER_LABEL));
    }

    #[gtk::test]
    fn gtk_ui_cancel_closes_the_pad_and_forgets_it() {
        let built = Built::new();
        let view = a_pad(&built);

        view.cancel.emit_clicked();

        assert!(built.viewer().state.borrow().signature.dialog.is_none());
    }

    #[test]
    fn the_pad_paints_without_a_display() {
        // Cairo needs no display: paint the pad onto an image surface so the
        // draw function is exercised on a machine with no GTK at all.
        let surface =
            cairo::ImageSurface::create(cairo::Format::ARgb32, 100, 50).expect("an image surface");
        let context = cairo::Context::new(&surface).expect("a context");
        let mut pad = Pad::default();
        pad.begin((5.0, 5.0));
        pad.extend((90.0, 40.0));
        pad.end();
        pad.begin((10.0, 10.0));
        pad.extend((20.0, 20.0));

        paint(&pad, &context, 100, 50);

        drop(context);
        let mut opaque = false;
        surface
            .with_data(|data| opaque = data.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255))
            .expect("the pixels are readable");
        assert!(opaque, "the pad is an opaque white sheet");
    }
}
