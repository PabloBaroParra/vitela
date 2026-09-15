//! The Extract dialog itself: the widgets, and the one handler that reads
//! them.
//!
//! Every rule the handler applies lives in [`super::options`], as a plain
//! function of its arguments. A rule worth a test should not need a widget
//! tree driven to reach it.
//!
//! ## Why a typed range and not a selection on the grid
//!
//! The Organize grid's cards are already carrying two gestures — a drag that
//! reorders and a set of per-card buttons that rotate and delete — and a
//! FlowBox in `SelectionMode::None` is what keeps those unambiguous. Adding a
//! third meaning to a click there is a change to the drag machinery, not a
//! change to this feature. The typed range is what `export` already asks for
//! and what the shared `pdf_save` grammar already parses, so the two screens
//! that ask "which pages?" ask it the same way.

use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, Button, Entry, Label, Orientation as GtkOrientation, Window,
};

use super::options::resolve_pages;

/// Asks which pages to pull out. Calls `submit` with ascending, zero-based
/// page indices once the answer parses; says why in its own error label when
/// it does not, so a mistyped range never costs the user the file chooser.
pub(super) fn prompt_for_pages<F>(
    window: &ApplicationWindow,
    total_pages: u32,
    on_cancel: impl Fn() + 'static,
    submit: F,
) where
    F: Fn(&ApplicationWindow, Vec<u32>) + 'static,
{
    // Shared by the Cancel button and the window's own close: `Window::
    // destroy` does not emit `close-request`, so the two ways out have to be
    // wired separately or pressing Cancel would leave the status line saying
    // whatever preceded the dialog.
    let on_cancel = Rc::new(on_cancel);

    let content = GtkBox::new(GtkOrientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let dialog = Window::builder()
        .transient_for(window)
        .modal(true)
        .title("Extract pages")
        .child(&content)
        .build();

    let pages_label = Label::new(Some("Pages to extract"));
    pages_label.set_xalign(0.0);
    let entry = Entry::builder().placeholder_text("1-3,7").build();
    entry.set_activates_default(true);

    // States the ceiling rather than leaving the user to discover it from a
    // refusal: the grammar's out-of-range message is correct but arrives
    // only after a wrong guess.
    let range_hint = Label::new(Some(&format!(
        "This document has {total_pages} page{}.",
        if total_pages == 1 { "" } else { "s" }
    )));
    range_hint.set_xalign(0.0);
    range_hint.add_css_class("dim-label");

    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    error_label.set_wrap(true);

    let buttons = GtkBox::new(GtkOrientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = Button::with_label("Cancel");
    let extract = Button::with_label("Extract");
    extract.add_css_class("home-primary");
    buttons.append(&cancel);
    buttons.append(&extract);

    content.append(&pages_label);
    content.append(&entry);
    content.append(&range_hint);
    content.append(&error_label);
    content.append(&buttons);

    extract.connect_clicked({
        let window = window.clone();
        let dialog = dialog.clone();
        let entry = entry.clone();
        let error_label = error_label.clone();
        move |_| match resolve_pages(entry.text().as_str(), total_pages) {
            Ok(pages) => {
                dialog.destroy();
                submit(&window, pages);
            }
            Err(message) => error_label.set_text(&message),
        }
    });

    cancel.connect_clicked({
        let dialog = dialog.clone();
        let on_cancel = on_cancel.clone();
        move |_| {
            dialog.destroy();
            on_cancel();
        }
    });

    // Backing out with the window button is the same answer as pressing
    // Cancel, so it produces the same status line rather than leaving
    // whatever sentence preceded the dialog on screen.
    dialog.connect_close_request(move |_| {
        on_cancel();
        gtk::glib::Propagation::Proceed
    });

    dialog.set_default_widget(Some(&extract));
    dialog.present();
    entry.grab_focus();
}
