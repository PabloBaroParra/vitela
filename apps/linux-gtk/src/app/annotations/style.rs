//! Annotation colour: converting GTK's normalized RGBA into the model's
//! RGB-only `Color`, and the dialog that asks for one.

use gtk::prelude::*;
use gtk::{gdk, ColorChooserDialog, ResponseType};
use pdf_document::{Annotation, AnnotationKind, Color, Command};

use crate::app::state::{SessionToken, Viewer};

use super::command::{apply_command, command, model};

/// Converts GTK's normalized RGB representation to the model's RGB-only color.
fn color_from_rgba(red: f64, green: f64, blue: f64, _alpha: f64) -> Option<Color> {
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

fn selected_color(annotation: &Annotation) -> Option<Color> {
    match annotation.kind {
        AnnotationKind::Highlight { color, .. }
        | AnnotationKind::Underline { color, .. }
        | AnnotationKind::Strikeout { color, .. }
        | AnnotationKind::Ink { color, .. }
        | AnnotationKind::Shape { color, .. } => Some(color),
        _ => None,
    }
}

pub(super) fn choose_restyle_color(viewer: &Viewer) {
    let (token, id, before, initial) = {
        let state = viewer.state.borrow();
        let Some(session) = state.session.as_ref() else {
            return;
        };
        let Some(id) = session.selected_annotation else {
            return;
        };
        let Some(before) = session
            .document_model
            .as_ref()
            .and_then(|document| document.annotations.get(id))
            .cloned()
        else {
            return;
        };
        let Some(initial) = selected_color(&before) else {
            return;
        };
        (
            SessionToken {
                generation: state.generation,
                edit_revision: session.edit_revision,
            },
            id,
            before,
            initial,
        )
    };
    let parent = viewer
        .status
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok());
    let dialog = ColorChooserDialog::new(Some("Choose annotation color"), parent.as_ref());
    dialog.set_use_alpha(false);
    dialog.set_rgba(&gdk::RGBA::new(
        f32::from(initial.r) / 255.0,
        f32::from(initial.g) / 255.0,
        f32::from(initial.b) / 255.0,
        1.0,
    ));
    dialog.connect_response({
        let viewer = viewer.clone();
        move |dialog, response| {
            let chosen = dialog.rgba();
            dialog.destroy();
            if response != ResponseType::Ok {
                return;
            }
            let Some(color) = color_from_rgba(
                f64::from(chosen.red()),
                f64::from(chosen.green()),
                f64::from(chosen.blue()),
                f64::from(chosen.alpha()),
            ) else {
                return;
            };
            // The dialog is modeless as far as the model is concerned: anything
            // could have replaced the document or edited it while it was open.
            let current = {
                let state = viewer.state.borrow();
                state
                    .session
                    .as_ref()
                    .is_some_and(|session| token.matches(state.generation, session.edit_revision))
            };
            if !current {
                return;
            }
            // `connect_response` is an `Fn`, so the command below cannot consume
            // the captured annotation — it works from a clone per response.
            let before = before.clone();
            command(&viewer, move |session| {
                let current = session
                    .document_model
                    .as_ref()
                    .and_then(|document| document.annotations.get(id));
                if current != Some(&before) {
                    return Ok("Color selection is no longer current.".to_string());
                }
                let mut after = before.clone();
                pdf_annotate::restyle_annotation(&mut after, color)
                    .map_err(|error| error.to_string())?;
                let document = model(session)?;
                apply_command(document, Command::ReplaceAnnotation { before, after });
                Ok("Annotation restyled. Changes are pending save.".to_string())
            });
        }
    });
    dialog.present();
}

pub(super) fn supports_restyle(annotation: &Annotation) -> bool {
    matches!(
        &annotation.kind,
        AnnotationKind::Highlight { .. }
            | AnnotationKind::Underline { .. }
            | AnnotationKind::Strikeout { .. }
            | AnnotationKind::Ink { .. }
            | AnnotationKind::Shape { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_conversion_rounds_rgb_and_ignores_alpha() {
        let converted = color_from_rgba(0.0, 0.5, 1.0, 0.05).expect("finite RGB converts");

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
        assert_eq!(color_from_rgba(f64::NAN, 0.5, 1.0, 1.0), None);
    }
}
