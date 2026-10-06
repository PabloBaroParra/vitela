//! The saved-signature offer: the signature remembered on this computer, shown
//! before a blank pad, with **Use** / **Draw new** / **Delete**.
//!
//! Shown on white, as the pad is — the ink is black, and a dark theme would
//! otherwise hide it.

use gtk::prelude::*;
use gtk::{cairo, Box as GtkBox, Button, DrawingArea, Orientation, Window};

use crate::app::selection::stamp_surface;
use crate::app::state::Viewer;

use super::flow;

const TITLE: &str = "Your signature";
const PREVIEW_WIDTH: i32 = 360;
const PREVIEW_HEIGHT: i32 = 160;
const PREVIEW_PADDING: f64 = 12.0;
const MARGIN: i32 = 12;
const UNSHOWABLE: &str = "This saved signature can no longer be shown.";

/// The offer dialog and its buttons.
#[derive(Clone)]
pub(super) struct OfferView {
    pub(super) window: Window,
    pub(super) use_button: Button,
    pub(super) draw_new: Button,
    pub(super) delete: Button,
    pub(super) cancel: Button,
}

/// Opens the offer for the remembered `png`, for a signature that will be
/// placed on `session_id`'s document.
pub(super) fn open(viewer: &Viewer, session_id: u64, png: Vec<u8>) -> OfferView {
    // Decoded once, here: a PNG of at most 1200 px is small enough to decode on
    // the frame that opens the dialog, and the draw function runs on every
    // frame after it.
    let picture = stamp_surface(png.clone());
    let can_show = picture.is_some();

    let preview = DrawingArea::new();
    preview.set_content_width(PREVIEW_WIDTH);
    preview.set_content_height(PREVIEW_HEIGHT);
    preview.set_hexpand(true);
    preview.update_property(&[gtk::accessible::Property::Label("Your saved signature")]);
    preview.set_draw_func(move |_, context, width, height| {
        paint(picture.as_ref(), context, width, height)
    });

    let delete = Button::with_label("Delete");
    let draw_new = Button::with_label("Draw new");
    let cancel = Button::with_label("Cancel");
    let use_button = Button::with_label("Use");
    use_button.add_css_class("suggested-action");
    use_button.set_sensitive(can_show);

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.append(&delete);
    buttons.append(&draw_new);
    buttons.append(&cancel);
    buttons.append(&use_button);

    let content = GtkBox::new(Orientation::Vertical, 8);
    content.set_margin_top(MARGIN);
    content.set_margin_bottom(MARGIN);
    content.set_margin_start(MARGIN);
    content.set_margin_end(MARGIN);
    content.append(&preview);
    content.append(&buttons);

    let mut dialog = Window::builder().modal(true).title(TITLE).child(&content);
    if let Some(window) = viewer.window() {
        dialog = dialog.transient_for(&window);
    }
    let window = dialog.build();
    flow::track(viewer, &window);

    let view = OfferView {
        window,
        use_button,
        draw_new,
        delete,
        cancel,
    };
    view.use_button.connect_clicked({
        let viewer = viewer.clone();
        let window = view.window.clone();
        move |_| flow::use_saved(&viewer, &window, session_id, png.clone())
    });
    view.draw_new.connect_clicked({
        let viewer = viewer.clone();
        let window = view.window.clone();
        move |_| flow::draw_new(&viewer, &window, session_id)
    });
    view.delete.connect_clicked({
        let viewer = viewer.clone();
        let window = view.window.clone();
        move |_| flow::delete_saved(&viewer, &window)
    });
    view.cancel.connect_clicked({
        let viewer = viewer.clone();
        let window = view.window.clone();
        move |_| flow::dismiss(&viewer, &window)
    });
    view.window.present();
    view
}

/// White sheet, signature fitted inside it and centred — or a plain apology
/// when the saved file is no longer a picture GTK can read.
fn paint(picture: Option<&cairo::ImageSurface>, context: &cairo::Context, width: i32, height: i32) {
    context.set_source_rgb(1.0, 1.0, 1.0);
    let _ = context.paint();
    let Some(picture) = picture else {
        context.set_source_rgb(0.0, 0.0, 0.0);
        context.move_to(PREVIEW_PADDING, f64::from(height) / 2.0);
        let _ = context.show_text(UNSHOWABLE);
        return;
    };
    let (picture_width, picture_height) = (f64::from(picture.width()), f64::from(picture.height()));
    if picture_width <= 0.0 || picture_height <= 0.0 {
        return;
    }
    let room_width = (f64::from(width) - 2.0 * PREVIEW_PADDING).max(1.0);
    let room_height = (f64::from(height) - 2.0 * PREVIEW_PADDING).max(1.0);
    // Fit, never enlarge: a small signature stays the size it will be drawn.
    let scale = (room_width / picture_width)
        .min(room_height / picture_height)
        .min(1.0);
    let _ = context.save();
    context.translate(
        (f64::from(width) - picture_width * scale) / 2.0,
        (f64::from(height) - picture_height * scale) / 2.0,
    );
    context.scale(scale, scale);
    let _ = context.set_source_surface(picture, 0.0, 0.0);
    let _ = context.paint();
    let _ = context.restore();
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::test_support::{a_png, open_document, the_session, Built};

    #[gtk::test]
    fn gtk_ui_the_offer_shows_the_saved_signature_with_use_draw_new_and_delete() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);

        let offer = open(viewer, the_session(viewer), a_png());

        assert_eq!(offer.window.title().as_deref(), Some(TITLE));
        assert_eq!(offer.use_button.label().as_deref(), Some("Use"));
        assert_eq!(offer.draw_new.label().as_deref(), Some("Draw new"));
        assert_eq!(offer.delete.label().as_deref(), Some("Delete"));
        assert!(offer.use_button.is_sensitive());
    }

    #[gtk::test]
    fn gtk_ui_a_saved_file_that_is_not_a_picture_cannot_be_used() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);

        let offer = open(viewer, the_session(viewer), b"not a png".to_vec());

        assert!(!offer.use_button.is_sensitive());
        assert!(offer.delete.is_sensitive(), "it can still be forgotten");
        assert!(offer.draw_new.is_sensitive());
    }

    #[test]
    fn the_offer_paints_a_picture_and_an_apology_without_a_display() {
        let surface =
            cairo::ImageSurface::create(cairo::Format::ARgb32, 60, 20).expect("an image surface");
        let canvas =
            cairo::ImageSurface::create(cairo::Format::ARgb32, 200, 100).expect("an image surface");
        let context = cairo::Context::new(&canvas).expect("a context");

        paint(Some(&surface), &context, 200, 100);
        paint(None, &context, 200, 100);
    }
}
