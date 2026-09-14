//! The Export images dialog itself: the widgets, and the one handler that
//! reads them.
//!
//! Every rule the handler applies — which pages a choice names, whether a page
//! can be rastered at the chosen resolution, what the status line says
//! afterwards — lives in `options`, as a plain function of its arguments. A
//! rule worth a test should not need three widgets driven to reach it, and a
//! widget tree is not where a reader should have to go looking for the
//! reason the DPI ceiling is 400.

use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, Button, CheckButton, DropDown, Entry, Label,
    Orientation as GtkOrientation, SpinButton, Window,
};

use super::options::{
    format_at, oversized_page, resolve_pages, ExportOptions, PageChoice, DEFAULT_DPI, FORMATS,
    MAX_DPI, MIN_DPI,
};

/// The dialog. Calls `submit` with the validated options once every answer is
/// usable; says why in its own error label when one is not, so a mistyped
/// range never costs the user the folder chooser.
pub(super) fn prompt_for_options<F>(
    window: &ApplicationWindow,
    page_sizes: Vec<(f32, f32)>,
    current_page: u32,
    on_cancel: impl Fn() + 'static,
    submit: F,
) where
    F: Fn(&ApplicationWindow, ExportOptions) + 'static,
{
    let total_pages = page_sizes.len() as u32;

    let content = GtkBox::new(GtkOrientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let dialog = Window::builder()
        .transient_for(window)
        .modal(true)
        .title("Export pages as images")
        .child(&content)
        .build();

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
    // The entry belongs to its radio button, so it wakes with it rather than
    // sitting live next to an unselected choice.
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

    let format_label = Label::new(Some("Format"));
    format_label.set_xalign(0.0);
    let names: Vec<&str> = FORMATS.iter().map(|&(name, _)| name).collect();
    let format = DropDown::from_strings(&names);

    let dpi_label = Label::new(Some("Resolution (DPI)"));
    dpi_label.set_xalign(0.0);
    let dpi = SpinButton::with_range(MIN_DPI, MAX_DPI, 1.0);
    dpi.set_value(DEFAULT_DPI);

    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    error_label.set_wrap(true);

    let buttons = GtkBox::new(GtkOrientation::Horizontal, 8);
    let cancel = Button::with_label("Cancel");
    let export = Button::with_label("Export");
    buttons.append(&cancel);
    buttons.append(&export);

    content.append(&pages_label);
    content.append(&all);
    content.append(&current);
    content.append(&custom_row);
    content.append(&format_label);
    content.append(&format);
    content.append(&dpi_label);
    content.append(&dpi);
    content.append(&error_label);
    content.append(&buttons);

    export.connect_clicked({
        let window = window.clone();
        let dialog = dialog.clone();
        let error_label = error_label.clone();
        let all = all.clone();
        let current = current.clone();
        let custom_entry = custom_entry.clone();
        let format = format.clone();
        let dpi = dpi.clone();
        move |_| {
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
            let dpi = dpi.value_as_int().max(1) as u32;
            if let Some(page) = oversized_page(&page_sizes, &pages, dpi) {
                error_label.set_text(&format!(
                    "Page {page} is too large to render at {dpi} DPI. Choose a lower resolution."
                ));
                return;
            }
            dialog.destroy();
            submit(
                &window,
                ExportOptions {
                    pages,
                    dpi,
                    format: format_at(format.selected()),
                },
            );
        }
    });
    cancel.connect_clicked({
        let dialog = dialog.clone();
        move |_| {
            on_cancel();
            dialog.destroy();
        }
    });

    dialog.present();
}
