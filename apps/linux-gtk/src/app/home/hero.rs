//! Home's welcome block and drop zone — the "start a task" half of the page.

use gtk::prelude::*;
use gtk::{
    gdk, Align, ApplicationWindow, Box as GtkBox, Button, DropTarget, GestureClick, Label,
    Orientation,
};

use crate::app::document::{open_file, show_file_chooser};
use crate::app::state::Viewer;

use super::is_pdf_path;

/// The CSS class that lights the zone up while a drag is over it. Applied on
/// enter and removed on both leave and drop, so a drag that ends elsewhere
/// cannot leave the zone stuck highlighted.
const DROP_ACTIVE: &str = "drop-active";

pub(crate) fn build_hero(window: &ApplicationWindow, viewer: &Viewer) -> GtkBox {
    let hero = GtkBox::new(Orientation::Vertical, 6);

    let title = Label::new(Some("Welcome to Vitela"));
    title.set_xalign(0.0);
    title.add_css_class("home-hero-title");
    hero.append(&title);

    let subtitle = Label::new(Some("Your fast, private PDF workspace."));
    subtitle.set_xalign(0.0);
    subtitle.add_css_class("home-hero-subtitle");
    subtitle.set_margin_bottom(10);
    hero.append(&subtitle);

    let heading = Label::new(Some("Start a new task"));
    heading.set_xalign(0.0);
    heading.add_css_class("home-section-title");
    heading.set_margin_bottom(6);
    hero.append(&heading);

    hero.append(&build_drop_zone(window, viewer));
    hero
}

fn build_drop_zone(window: &ApplicationWindow, viewer: &Viewer) -> GtkBox {
    let zone = GtkBox::new(Orientation::Vertical, 10);
    zone.add_css_class("home-dropzone");
    zone.set_halign(Align::Fill);

    let prompt = Label::new(Some("Drag and drop a PDF here"));
    prompt.add_css_class("home-hero-subtitle");
    zone.append(&prompt);

    let hint = Label::new(Some("or click anywhere in this area to open one"));
    hint.add_css_class("recent-meta");
    zone.append(&hint);

    let choose = Button::with_label("Select file");
    choose.add_css_class("home-primary");
    choose.set_halign(Align::Center);
    choose.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| show_file_chooser(&window, &viewer)
    });
    zone.append(&choose);

    // The zone is a box, so it has no click behaviour of its own — the hint
    // above promises one, and a gesture is what keeps that promise without
    // making the whole area a `Button` (which would swallow the drop target
    // and turn every drag-over into a pressed state).
    let click = GestureClick::new();
    click.connect_released({
        let window = window.clone();
        let viewer = viewer.clone();
        let zone_for_pick = zone.clone();
        let choose = choose.clone();
        move |_, _, x, y| {
            // The button inside runs its own handler. Relying on GtkButton
            // claiming the gesture sequence first would probably be enough,
            // but "probably" here costs a second file chooser stacked on the
            // first — the same failure `document::begin_loading` already had
            // to fix for password prompts. Asking which widget is under the
            // release makes it a fact instead.
            if on_the_button(&zone_for_pick, &choose, x, y) {
                return;
            }
            show_file_chooser(&window, &viewer);
        }
    });
    zone.add_controller(click);

    let target = DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    target.connect_enter({
        let zone = zone.clone();
        move |_, _, _| {
            zone.add_css_class(DROP_ACTIVE);
            gdk::DragAction::COPY
        }
    });
    target.connect_leave({
        let zone = zone.clone();
        move |_| zone.remove_css_class(DROP_ACTIVE)
    });
    target.connect_drop({
        let zone = zone.clone();
        let viewer = viewer.clone();
        move |_, value, _, _| {
            zone.remove_css_class(DROP_ACTIVE);
            accept_drop(&viewer, value)
        }
    });
    zone.add_controller(target);

    zone
}

/// Whether the point (`x`, `y`), in `zone`'s coordinates, lands on `button`
/// or on something inside it.
fn on_the_button(zone: &GtkBox, button: &Button, x: f64, y: f64) -> bool {
    let Some(picked) = zone.pick(x, y, gtk::PickFlags::DEFAULT) else {
        return false;
    };
    picked.eq(button.upcast_ref::<gtk::Widget>()) || picked.is_ancestor(button)
}

/// Opens the first dropped file, or reports why it was refused.
///
/// Narrower than `input::connect_window_file_drop`, which also accepts images
/// because it covers the *page* canvas and an image dropped there becomes a
/// stamp. Home has no page to stamp onto, so anything but a PDF is refused
/// here rather than silently doing nothing.
fn accept_drop(viewer: &Viewer, value: &gtk::glib::Value) -> bool {
    let Ok(files) = value.get::<gdk::FileList>() else {
        return false;
    };
    let Some(file) = files.files().into_iter().next() else {
        return false;
    };
    let Some(path) = file.path() else {
        viewer
            .status
            .set_text("Only local files can be dropped here.");
        return false;
    };
    if !is_pdf_path(&path) {
        viewer
            .status
            .set_text("Only PDF files can be opened from the Home screen.");
        return false;
    }
    open_file(viewer, path);
    true
}
