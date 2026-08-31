//! The style inspector for the selected form field (T-141): font family,
//! size, and color. Three live controls rather than the single restyle
//! dialog `annotations::style` offers, because a field's `TextStyle` has
//! three independently meaningful parts a user reasonably wants to change
//! one at a time, where an annotation's own restyle is only ever a color.

use gtk::prelude::*;
use gtk::{gdk, ColorDialog};
use pdf_document::{Color, Command, FontFamily, TextStyle};

use crate::app::state::{SessionToken, Viewer};

use super::command::{apply_command, command, model};
use super::SELECTION_GONE;

fn font_from_selected(index: u32) -> FontFamily {
    match index {
        1 => FontFamily::TimesRoman,
        2 => FontFamily::Courier,
        _ => FontFamily::Helvetica,
    }
}

fn selected_from_font(font: FontFamily) -> u32 {
    match font {
        FontFamily::Helvetica => 0,
        FontFamily::TimesRoman => 1,
        FontFamily::Courier => 2,
    }
}

/// Converts GTK's normalized RGB representation to the model's RGB-only
/// color — identical to `annotations::style::color_from_rgba`, duplicated
/// rather than shared: it is three lines, and reaching across to a sibling
/// feature module for them would be a stranger dependency than repeating
/// them.
fn color_from_rgba(red: f64, green: f64, blue: f64) -> Option<Color> {
    fn channel(value: f64) -> Option<u8> {
        value
            .is_finite()
            .then(|| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    Some(Color {
        r: channel(red)?,
        g: channel(green)?,
        b: channel(blue)?,
    })
}

/// Wires the font/size/color controls. Each one restyles the selected field
/// immediately on change — no separate "Apply" step, matching every other
/// direct-manipulation control in this shell (a drag, a nudge, a color
/// pick).
pub(crate) fn connect_style_controls(viewer: &Viewer) {
    viewer.forms.font.connect_selected_notify({
        let viewer = viewer.clone();
        move |dropdown| {
            if viewer.forms.syncing.get() {
                return;
            }
            let font = font_from_selected(dropdown.selected());
            restyle_selected(&viewer, move |style| style.font = font);
        }
    });
    viewer.forms.size.connect_value_changed({
        let viewer = viewer.clone();
        move |spin| {
            if viewer.forms.syncing.get() {
                return;
            }
            let size_pt = spin.value();
            restyle_selected(&viewer, move |style| style.size_pt = size_pt);
        }
    });
    viewer.forms.color.connect_clicked({
        let viewer = viewer.clone();
        move |_| choose_field_color(&viewer)
    });
}

/// Refreshes the inspector from the selected field's style, or blanks it
/// when nothing is selected — called from `toolbar::update_forms_controls`
/// after every selection change, restyle, undo/redo, and document
/// open/close.
///
/// `syncing` stops the write-back below from being mistaken for a user edit
/// and recording a spurious restyle — see `FormFieldToolbar::syncing`'s own
/// doc.
pub(super) fn refresh(viewer: &Viewer, style: Option<TextStyle>, enabled: bool) {
    viewer.forms.syncing.set(true);
    if let Some(style) = style {
        viewer
            .forms
            .font
            .set_selected(selected_from_font(style.font));
        viewer.forms.size.set_value(style.size_pt);
    }
    viewer.forms.syncing.set(false);
    viewer.forms.font.set_sensitive(enabled && style.is_some());
    viewer.forms.size.set_sensitive(enabled && style.is_some());
    viewer.forms.color.set_sensitive(enabled && style.is_some());
}

fn restyle_selected(viewer: &Viewer, apply: impl FnOnce(&mut TextStyle) + 'static) {
    let Some(id) = viewer
        .state
        .borrow()
        .session
        .as_ref()
        .and_then(|session| session.selected_form_field)
    else {
        return;
    };
    command(viewer, move |session| {
        let document = model(session)?;
        let before = document
            .form_fields
            .get(id)
            .map(|field| field.style)
            .ok_or_else(|| SELECTION_GONE.to_string())?;
        let mut after = before;
        apply(&mut after);
        apply_command(
            document,
            Command::RestyleFormField {
                id,
                from: before,
                to: after,
            },
        );
        Ok("Field restyled. Changes are pending save.".to_string())
    });
}

/// Opens a color picker for the selected field's style, mirroring
/// `annotations::style::choose_restyle_color`'s async shape: the dialog is
/// modeless as far as the model is concerned, so the response re-validates
/// against a captured `SessionToken` and the field's own `before` snapshot
/// before recording anything.
fn choose_field_color(viewer: &Viewer) {
    let (token, id, before) = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            return;
        };
        let Some(id) = session.selected_form_field else {
            return;
        };
        let Some(before) = session
            .document_model
            .as_ref()
            .and_then(|document| document.form_fields.get(id))
            .map(|field| field.style)
        else {
            return;
        };
        (
            SessionToken {
                generation: state.generation,
                edit_revision: session.edit_revision,
            },
            id,
            before,
        )
    };
    let parent = viewer
        .status
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok());
    let dialog = ColorDialog::builder()
        .title("Choose field color")
        .modal(true)
        .with_alpha(false)
        .build();
    let initial = gdk::RGBA::new(
        f32::from(before.color.r) / 255.0,
        f32::from(before.color.g) / 255.0,
        f32::from(before.color.b) / 255.0,
        1.0,
    );
    dialog.choose_rgba(
        parent.as_ref(),
        Some(&initial),
        None::<&gtk::gio::Cancellable>,
        {
            let viewer = viewer.clone();
            move |result| {
                let Ok(chosen) = result else {
                    return;
                };
                let Some(color) = color_from_rgba(
                    f64::from(chosen.red()),
                    f64::from(chosen.green()),
                    f64::from(chosen.blue()),
                ) else {
                    return;
                };
                let current = {
                    let state = viewer.state.borrow();
                    state.session.as_ref().is_some_and(|session| {
                        token.matches(state.generation, session.edit_revision)
                    })
                };
                if !current {
                    return;
                }
                command(&viewer, move |session| {
                    let document = model(session)?;
                    let still_current = document
                        .form_fields
                        .get(id)
                        .is_some_and(|field| field.style == before);
                    if !still_current {
                        return Ok("Color selection is no longer current.".to_string());
                    }
                    let mut after = before;
                    after.color = color;
                    apply_command(
                        document,
                        Command::RestyleFormField {
                            id,
                            from: before,
                            to: after,
                        },
                    );
                    Ok("Field restyled. Changes are pending save.".to_string())
                });
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_standard_14_font_round_trips_through_its_dropdown_index() {
        for font in [
            FontFamily::Helvetica,
            FontFamily::TimesRoman,
            FontFamily::Courier,
        ] {
            assert_eq!(font_from_selected(selected_from_font(font)), font);
        }
    }

    #[test]
    fn an_unknown_index_falls_back_to_helvetica() {
        assert_eq!(font_from_selected(99), FontFamily::Helvetica);
    }

    #[test]
    fn rgba_conversion_rounds_rgb() {
        let converted = color_from_rgba(0.0, 0.5, 1.0).expect("finite RGB converts");

        assert_eq!(
            converted,
            Color {
                r: 0,
                g: 128,
                b: 255
            }
        );
    }

    #[test]
    fn rgba_conversion_rejects_non_finite_channels() {
        assert_eq!(color_from_rgba(f64::NAN, 0.5, 1.0), None);
    }
}
