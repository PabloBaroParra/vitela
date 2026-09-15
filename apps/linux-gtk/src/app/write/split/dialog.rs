//! The Split dialog itself: the widgets, and the one handler that reads them.
//!
//! Every rule the handler applies lives in [`super::options`], as a plain
//! function of its arguments. A rule worth a test should not need a widget
//! tree driven to reach it.
//!
//! ## Why "split after page N" and not "into N equal parts"
//!
//! Both are real questions, and this is the one that can answer the other:
//! cut points describe any split, including the even one, while "every N
//! pages" describes only the even one. The grammar is also already in the
//! user's hands — it is `pdf_save::parse_page_selection`, the same thing
//! Export and Extract ask in — so the three screens that ask "which pages?"
//! ask it the same way rather than in three dialects.

use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, Button, Entry, Label, Orientation as GtkOrientation, Window,
};

use super::options::resolve_cuts;

/// Asks where to cut. Calls `submit` with ascending, zero-based indices of the
/// pages each cut falls *after* once the answer parses; says why in its own
/// error label when it does not, so a mistyped list never costs the user the
/// folder chooser.
pub(super) fn prompt_for_cuts<F>(
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
        .title("Split PDF")
        .child(&content)
        .build();

    let cuts_label = Label::new(Some("Split after page"));
    cuts_label.set_xalign(0.0);
    let entry = Entry::builder().placeholder_text("3,7").build();
    entry.set_activates_default(true);

    // States both the ceiling and what the answer will produce, rather than
    // leaving the user to discover either from a refusal: the grammar's
    // out-of-range message is correct but arrives only after a wrong guess,
    // and "one more file than cuts" is not obvious from the label alone.
    let range_hint = Label::new(Some(&format!(
        "This document has {total_pages} pages. Each cut starts a new file.",
    )));
    range_hint.set_xalign(0.0);
    range_hint.set_wrap(true);
    range_hint.add_css_class("dim-label");

    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    error_label.set_wrap(true);

    let buttons = GtkBox::new(GtkOrientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = Button::with_label("Cancel");
    let split = Button::with_label("Split");
    split.add_css_class("home-primary");
    buttons.append(&cancel);
    buttons.append(&split);

    content.append(&cuts_label);
    content.append(&entry);
    content.append(&range_hint);
    content.append(&error_label);
    content.append(&buttons);

    split.connect_clicked({
        let window = window.clone();
        let dialog = dialog.clone();
        let entry = entry.clone();
        let error_label = error_label.clone();
        move |_| match resolve_cuts(entry.text().as_str(), total_pages) {
            Ok(cuts) => {
                dialog.destroy();
                submit(&window, cuts);
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

    dialog.set_default_widget(Some(&split));
    dialog.present();
    entry.grab_focus();
}
