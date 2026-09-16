//! The Compress dialog: three presets, the size the document is now, and the
//! one handler that reads the answer.
//!
//! Every rule it applies lives in [`options`](super::options) as a plain
//! function — the words each preset is offered under, the default it opens on,
//! how a byte count is written. What is here is the widget tree and the
//! grouping, which is all a reader should have to look at to know what the
//! dialog does.
//!
//! ## There is no predicted saving, on purpose
//!
//! The obvious third column of a preset list is "about 40% smaller", and this
//! dialog does not have one. Nothing in `pdf-compress` can answer it without
//! running: the gain depends on how the file was written, how many of its
//! images are already below the preset's ceiling, and how much of it is
//! duplicated — T-193 measured a corpus where *no* image on any page exceeds
//! even `Small`'s 96 DPI, and T-196 measured a document whose entire saving
//! was write format rather than content. A percentage put here would be a
//! guess printed in the same typeface as a measurement.
//!
//! What the dialog shows instead is the one number it knows: the size of the
//! file on disk right now. The real figures arrive on the status line the
//! moment the compression finishes, and the destination is only asked for
//! after the user has seen them.

use gtk::prelude::*;
use gtk::{
    Align, ApplicationWindow, Box as GtkBox, Button, CheckButton, Label,
    Orientation as GtkOrientation, Window,
};
use pdf_compress::CompressPreset;

use super::options::{human_size, preset_words, DEFAULT_PRESET};

/// Asks which preset to compress with. Calls `submit` with the chosen one,
/// `on_cancel` when the user backs out.
pub(super) fn prompt_for_preset<F>(
    window: &ApplicationWindow,
    current_size: u64,
    on_cancel: impl Fn() + 'static,
    submit: F,
) where
    F: Fn(&ApplicationWindow, CompressPreset) + 'static,
{
    let content = GtkBox::new(GtkOrientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let dialog = Window::builder()
        .transient_for(window)
        .modal(true)
        .title("Compress PDF")
        .child(&content)
        .build();

    let current = Label::new(Some(&format!(
        "This file is {} on disk.",
        human_size(current_size)
    )));
    current.set_xalign(0.0);
    current.set_wrap(true);
    content.append(&current);

    // Built from `CompressPreset::all` rather than from three literals here,
    // so the list is what the core offers rather than what this file
    // remembers being offered.
    let mut choices: Vec<(CompressPreset, CheckButton)> = Vec::new();
    for preset in CompressPreset::all() {
        let (name, description) = preset_words(preset);
        let radio = CheckButton::with_label(name);
        if let Some((_, first)) = choices.first() {
            radio.set_group(Some(first));
        }
        radio.set_active(preset == DEFAULT_PRESET);
        content.append(&radio);

        let caption = Label::new(Some(description));
        caption.set_xalign(0.0);
        caption.set_wrap(true);
        // Indented under the radio it belongs to, so the three rows read as
        // three choices rather than as six lines of alternating weight.
        caption.set_margin_start(24);
        caption.set_margin_bottom(4);
        caption.add_css_class("dim-label");
        content.append(&caption);

        choices.push((preset, radio));
    }

    let buttons = GtkBox::new(GtkOrientation::Horizontal, 8);
    buttons.set_halign(Align::End);
    let cancel = Button::with_label("Cancel");
    let compress = Button::with_label("Compress");
    buttons.append(&cancel);
    buttons.append(&compress);
    content.append(&buttons);

    compress.connect_clicked({
        let window = window.clone();
        let dialog = dialog.clone();
        move |_| {
            let preset = chosen_preset(&choices);
            dialog.destroy();
            submit(&window, preset);
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

/// The preset whose radio is active, falling back to the default.
///
/// The fallback is a real branch rather than defensive noise: a `CheckButton`
/// group can be left with nothing active by an untoggle, and the dialog must
/// still answer with a preset the user was shown rather than refuse to close.
fn chosen_preset(choices: &[(CompressPreset, CheckButton)]) -> CompressPreset {
    choices
        .iter()
        .find(|(_, radio)| radio.is_active())
        .map_or(DEFAULT_PRESET, |&(preset, _)| preset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_group() -> Vec<(CompressPreset, CheckButton)> {
        let mut choices: Vec<(CompressPreset, CheckButton)> = Vec::new();
        for preset in CompressPreset::all() {
            let radio = CheckButton::new();
            if let Some((_, first)) = choices.first() {
                radio.set_group(Some(first));
            }
            choices.push((preset, radio));
        }
        choices
    }

    #[gtk::test]
    fn gtk_ui_the_active_radio_is_the_preset_that_is_submitted() {
        let choices = a_group();
        choices[2].1.set_active(true);

        assert_eq!(chosen_preset(&choices), CompressPreset::Small);
    }

    /// Nothing active is possible in a `CheckButton` group, and the answer has
    /// to be the preset the dialog opened on rather than a refusal to close.
    #[gtk::test]
    fn gtk_ui_a_group_with_nothing_active_answers_with_the_default() {
        assert_eq!(chosen_preset(&a_group()), DEFAULT_PRESET);
    }
}
