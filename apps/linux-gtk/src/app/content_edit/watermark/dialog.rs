//! The Add watermark dialog itself: the widgets, and the one handler that
//! reads them. Every rule it applies lives in `options`.
//!
//! One dialog for both kinds of mark. Text and picture share everything but
//! the mark itself — pages, opacity, angle, layer — so a "Text / Picture"
//! switch shows one of two small sections rather than splitting the tool
//! into two tiles that would ask the same five questions twice.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gio, ApplicationWindow, Box as GtkBox, Button, CheckButton, Entry, FileDialog, Label,
    Orientation as GtkOrientation, SpinButton, Window,
};
use pdf_document::{Watermark, WatermarkLayer};

use super::options::{
    default_opacity_percent, default_scale_percent, fraction_from_percent, look_defaults,
    resolve_pages, resolve_picture, resolve_text, MarkKind, MarkOptions, PageChoice,
    WatermarkOptions, DEFAULT_TEXT, MAX_ANGLE, MAX_FONT_SIZE, MAX_OPACITY_PERCENT,
    MAX_SCALE_PERCENT, MIN_ANGLE, MIN_FONT_SIZE, MIN_OPACITY_PERCENT, MIN_SCALE_PERCENT,
};

const NO_PICTURE: &str = "No picture chosen";

/// The dialog. Calls `submit` with the validated options; the dialog closes
/// only when `submit` answers `Ok`. An `Err` — text the stamp's font cannot
/// show, a file that is not a picture, a document that changed underneath —
/// lands in the dialog's own error label, so the user can fix that one
/// answer without redoing the rest.
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

    let kind_label = left_label("Stamp");
    let text_kind = CheckButton::with_label("Text");
    text_kind.set_active(true);
    let picture_kind = CheckButton::with_label("Picture");
    picture_kind.set_group(Some(&text_kind));
    let kind_row = GtkBox::new(GtkOrientation::Horizontal, 8);
    kind_row.append(&text_kind);
    kind_row.append(&picture_kind);

    // Text section.
    let text_section = GtkBox::new(GtkOrientation::Vertical, 8);
    let text = Entry::builder().text(DEFAULT_TEXT).build();
    let size = SpinButton::with_range(MIN_FONT_SIZE, MAX_FONT_SIZE, 1.0);
    size.set_value(Watermark::DEFAULT_FONT_SIZE);
    text_section.append(&left_label("Text"));
    text_section.append(&text);
    text_section.append(&left_label("Size (pt)"));
    text_section.append(&size);

    // Picture section — hidden until "Picture" is chosen.
    let picture_section = GtkBox::new(GtkOrientation::Vertical, 8);
    picture_section.set_visible(false);
    let choose = Button::with_label("Choose picture…");
    let picture_name = Label::new(Some(NO_PICTURE));
    picture_name.set_xalign(0.0);
    picture_name.set_hexpand(true);
    picture_name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    let picture_row = GtkBox::new(GtkOrientation::Horizontal, 8);
    picture_row.append(&choose);
    picture_row.append(&picture_name);
    let scale = SpinButton::with_range(MIN_SCALE_PERCENT, MAX_SCALE_PERCENT, 5.0);
    scale.set_value(default_scale_percent());
    picture_section.append(&left_label("Picture (PNG or JPEG)"));
    picture_section.append(&picture_row);
    picture_section.append(&left_label("Size (% of page)"));
    picture_section.append(&scale);
    // The picked file's bytes, read once when it is chosen: the dialog then
    // holds exactly what will be recorded, whatever happens to the file.
    let picked: Rc<RefCell<Option<Vec<u8>>>> = Rc::new(RefCell::new(None));

    let pages_label = left_label("Pages");
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

    let opacity_label = left_label("Opacity (%)");
    let opacity = SpinButton::with_range(MIN_OPACITY_PERCENT, MAX_OPACITY_PERCENT, 5.0);
    opacity.set_value(default_opacity_percent());

    let angle_label = left_label("Angle (degrees)");
    let angle = SpinButton::with_range(MIN_ANGLE, MAX_ANGLE, 15.0);

    let layer_label = left_label("Position");
    let over = CheckButton::with_label("Over the page content");
    let under = CheckButton::with_label("Behind the page content");
    under.set_group(Some(&over));

    let set_look = {
        let angle = angle.clone();
        let over = over.clone();
        let under = under.clone();
        move |kind: MarkKind| {
            let (degrees, layer) = look_defaults(kind);
            angle.set_value(degrees);
            match layer {
                WatermarkLayer::OverContent => over.set_active(true),
                WatermarkLayer::UnderContent => under.set_active(true),
            }
        }
    };
    set_look(MarkKind::Text);

    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    error_label.set_wrap(true);

    // Watching one radio of the pair is enough: it toggles on both switches.
    picture_kind.connect_toggled({
        let text_section = text_section.clone();
        let picture_section = picture_section.clone();
        let error_label = error_label.clone();
        let choose = choose.clone();
        let text = text.clone();
        move |button| {
            let kind = if button.is_active() {
                MarkKind::Picture
            } else {
                MarkKind::Text
            };
            text_section.set_visible(kind == MarkKind::Text);
            picture_section.set_visible(kind == MarkKind::Picture);
            set_look(kind);
            // A refusal about the other kind's mark no longer applies.
            error_label.set_text("");
            match kind {
                MarkKind::Text => text.grab_focus(),
                MarkKind::Picture => choose.grab_focus(),
            };
        }
    });

    choose.connect_clicked({
        let dialog = dialog.clone();
        let picked = picked.clone();
        let picture_name = picture_name.clone();
        let error_label = error_label.clone();
        move |_| {
            let chooser = FileDialog::builder()
                .title("Choose watermark picture")
                .accept_label("Choose")
                .default_filter(&crate::app::content_edit::image::picture_filter())
                .build();
            chooser.open(Some(&dialog), None::<&gio::Cancellable>, {
                let picked = picked.clone();
                let picture_name = picture_name.clone();
                let error_label = error_label.clone();
                move |result| {
                    // Dismissing the chooser keeps whatever was picked before.
                    let Ok(file) = result else {
                        return;
                    };
                    let Some(path) = file.path() else {
                        error_label.set_text("The selected location is not a local file.");
                        return;
                    };
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            picked.replace(Some(bytes));
                            let name = path
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.display().to_string());
                            picture_name.set_text(&name);
                            error_label.set_text("");
                        }
                        Err(error) => error_label
                            .set_text(&format!("Could not read {}: {error}", path.display())),
                    }
                }
            });
        }
    });

    let buttons = GtkBox::new(GtkOrientation::Horizontal, 8);
    let cancel = Button::with_label("Cancel");
    let add = Button::with_label("Add watermark");
    buttons.append(&cancel);
    buttons.append(&add);

    content.append(&kind_label);
    content.append(&kind_row);
    content.append(&text_section);
    content.append(&picture_section);
    content.append(&pages_label);
    content.append(&all);
    content.append(&current);
    content.append(&custom_row);
    content.append(&opacity_label);
    content.append(&opacity);
    content.append(&angle_label);
    content.append(&angle);
    content.append(&layer_label);
    content.append(&over);
    content.append(&under);
    content.append(&error_label);
    content.append(&buttons);

    let confirm = {
        let dialog = dialog.clone();
        let error_label = error_label.clone();
        let text = text.clone();
        let picture_kind = picture_kind.clone();
        let picked = picked.clone();
        let all = all.clone();
        let current = current.clone();
        let custom_entry = custom_entry.clone();
        move || {
            let mark = if picture_kind.is_active() {
                resolve_picture(picked.borrow().as_deref()).map(|bytes| MarkOptions::Picture {
                    bytes,
                    scale: fraction_from_percent(scale.value()),
                })
            } else {
                resolve_text(text.text().as_str()).map(|typed| MarkOptions::Text {
                    text: typed,
                    font_size: size.value(),
                })
            };
            let mark = match mark {
                Ok(mark) => mark,
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
                mark,
                opacity: fraction_from_percent(opacity.value()),
                rotation_degrees: angle.value(),
                layer: if under.is_active() {
                    WatermarkLayer::UnderContent
                } else {
                    WatermarkLayer::OverContent
                },
            };
            match submit(options) {
                Ok(()) => dialog.destroy(),
                Err(message) => error_label.set_text(&message),
            }
        }
    };
    let confirm = Rc::new(confirm);

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

fn left_label(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.set_xalign(0.0);
    label
}
