//! The Add watermark dialog itself: the widgets, and the one handler that
//! reads them. Every rule it applies lives in `options`.

use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, Button, CheckButton, Entry, Label,
    Orientation as GtkOrientation, SpinButton, Window,
};
use pdf_document::Watermark;

use super::options::{
    default_opacity_percent, opacity_from_percent, resolve_pages, resolve_text, PageChoice,
    WatermarkOptions, DEFAULT_TEXT, MAX_ANGLE, MAX_FONT_SIZE, MAX_OPACITY_PERCENT, MIN_ANGLE,
    MIN_FONT_SIZE, MIN_OPACITY_PERCENT,
};

/// The dialog. Calls `submit` with the validated options; the dialog closes
/// only when `submit` answers `Ok`. An `Err` — text the stamp's font cannot
/// show, a document that changed underneath — lands in the dialog's own
/// error label, so the user can fix the text without retyping everything.
pub(super) fn prompt_for_options<F>(
    window: &ApplicationWindow,
    total_pages: u32,
    current_page: u32,
    on_cancel: impl Fn() + 'static,
    submit: F,
) where
    F: Fn(WatermarkOptions) -> Result<(), String> + 'static,
{
    let content = GtkBox::new(GtkOrientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let dialog = Window::builder()
        .transient_for(window)
        .modal(true)
        .title("Add watermark")
        .child(&content)
        .build();

    let text_label = Label::new(Some("Text"));
    text_label.set_xalign(0.0);
    let text = Entry::builder().text(DEFAULT_TEXT).build();

    let pages_label = Label::new(Some("Pages"));
    pages_label.set_xalign(0.0);
    let all = CheckButton::with_label("All pages");
    all.set_active(true);
    let current = CheckButton::with_label("Current page");
    current.set_group(Some(&all));
    let custom = CheckButton::with_label("Pages");
    custom.set_group(Some(&all));
    let custom_entry = Entry::builder()
        .placeholder_text("1-3,7")
        .sensitive(false)
        .build();
    // Same as Export images: the entry wakes with its radio button.
    custom.connect_toggled({
        let custom_entry = custom_entry.clone();
        move |button| {
            custom_entry.set_sensitive(button.is_active());
            if button.is_active() {
                custom_entry.grab_focus();
            }
        }
    });
    let custom_row = GtkBox::new(GtkOrientation::Horizontal, 8);
    custom_row.append(&custom);
    custom_row.append(&custom_entry);

    let size_label = Label::new(Some("Size (pt)"));
    size_label.set_xalign(0.0);
    let size = SpinButton::with_range(MIN_FONT_SIZE, MAX_FONT_SIZE, 1.0);
    size.set_value(Watermark::DEFAULT_FONT_SIZE);

    let opacity_label = Label::new(Some("Opacity (%)"));
    opacity_label.set_xalign(0.0);
    let opacity = SpinButton::with_range(MIN_OPACITY_PERCENT, MAX_OPACITY_PERCENT, 5.0);
    opacity.set_value(default_opacity_percent());

    let angle_label = Label::new(Some("Angle (degrees)"));
    angle_label.set_xalign(0.0);
    let angle = SpinButton::with_range(MIN_ANGLE, MAX_ANGLE, 15.0);
    angle.set_value(Watermark::DEFAULT_ROTATION_DEGREES);

    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    error_label.set_wrap(true);

    let buttons = GtkBox::new(GtkOrientation::Horizontal, 8);
    let cancel = Button::with_label("Cancel");
    let add = Button::with_label("Add watermark");
    buttons.append(&cancel);
    buttons.append(&add);

    content.append(&text_label);
    content.append(&text);
    content.append(&pages_label);
    content.append(&all);
    content.append(&current);
    content.append(&custom_row);
    content.append(&size_label);
    content.append(&size);
    content.append(&opacity_label);
    content.append(&opacity);
    content.append(&angle_label);
    content.append(&angle);
    content.append(&error_label);
    content.append(&buttons);

    let confirm = {
        let dialog = dialog.clone();
        let error_label = error_label.clone();
        let text = text.clone();
        let all = all.clone();
        let current = current.clone();
        let custom_entry = custom_entry.clone();
        move || {
            let typed = match resolve_text(text.text().as_str()) {
                Ok(typed) => typed,
                Err(message) => {
                    error_label.set_text(&message);
                    return;
                }
            };
            let choice = if all.is_active() {
                PageChoice::All
            } else if current.is_active() {
                PageChoice::Current
            } else {
                PageChoice::Custom
            };
            let pages = match resolve_pages(
                choice,
                custom_entry.text().as_str(),
                current_page,
                total_pages,
            ) {
                Ok(pages) => pages,
                Err(message) => {
                    error_label.set_text(&message);
                    return;
                }
            };
            let options = WatermarkOptions {
                pages,
                text: typed,
                font_size: size.value(),
                opacity: opacity_from_percent(opacity.value()),
                rotation_degrees: angle.value(),
            };
            match submit(options) {
                Ok(()) => dialog.destroy(),
                Err(message) => error_label.set_text(&message),
            }
        }
    };
    let confirm = std::rc::Rc::new(confirm);

    add.connect_clicked({
        let confirm = confirm.clone();
        move |_| confirm()
    });
    // Enter in the text entry stamps, the same as every other one-field
    // prompt in this shell; the page and look controls keep their defaults.
    text.connect_activate({
        let confirm = confirm.clone();
        move |_| confirm()
    });
    cancel.connect_clicked({
        let dialog = dialog.clone();
        move |_| {
            on_cancel();
            dialog.destroy();
        }
    });

    dialog.present();
    text.grab_focus();
    text.select_region(0, -1);
}
