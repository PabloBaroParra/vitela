//! The forms toolbar (T-141): the "Edit forms" mode toggle, the four
//! placement toggles, and the style inspector for the selected field —
//! mirrors `annotations::toolbar`'s shape (`FlowBox`-in-`ScrolledWindow`
//! rows, one function owning every sensitivity rule) applied to
//! `document.form_fields` instead of `document.annotations`.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    Adjustment, Box as GtkBox, Button, DropDown, FlowBox, Orientation, PolicyType, ScrolledWindow,
    SelectionMode, SpinButton, StringList, ToggleButton,
};

use crate::app::state::{FieldKind, FormFieldToolbar, Viewer};
use crate::app::tools_panel::panel_heading;

use super::style::{connect_style_controls, refresh as refresh_style};

/// Builds the forms toolbar and the "Fill & Sign" page content it lives on.
///
/// Two `FlowBox` rows, not one flat `GtkBox`, for the same load-bearing
/// reason `annotations::add_annotation_toolbar` documents at length: any
/// horizontal row of labelled controls in this shell has to be able to wrap,
/// or it becomes the window's own minimum width. Do not swap either row for
/// a bare box.
pub(crate) fn build_forms_content() -> (FormFieldToolbar, GtkBox) {
    let place_flow = FlowBox::new();
    place_flow.set_selection_mode(SelectionMode::None);
    place_flow.set_row_spacing(4);
    place_flow.set_column_spacing(4);
    place_flow.set_homogeneous(false);

    let mode = ToggleButton::with_label("Edit forms");
    mode.set_sensitive(false);
    place_flow.append(&mode);

    let place: Vec<(FieldKind, ToggleButton)> = FieldKind::ALL
        .iter()
        .map(|&kind| {
            let button = ToggleButton::with_label(kind.label());
            button.set_sensitive(false);
            place_flow.append(&button);
            (kind, button)
        })
        .collect();

    let place_row = ScrolledWindow::builder()
        .child(&place_flow)
        .hscrollbar_policy(PolicyType::Automatic)
        .vscrollbar_policy(PolicyType::Never)
        .propagate_natural_height(true)
        .build();

    let style_flow = FlowBox::new();
    style_flow.set_selection_mode(SelectionMode::None);
    style_flow.set_row_spacing(4);
    style_flow.set_column_spacing(4);
    style_flow.set_homogeneous(false);

    let fonts = StringList::new(&["Helvetica", "Times Roman", "Courier"]);
    let font = DropDown::new(Some(fonts), None::<gtk::Expression>);
    font.set_sensitive(false);
    style_flow.append(&font);

    // 4pt–400pt covers everything `pdf-form`'s `/DA` round-trip supports;
    // there is no upper bound in the model itself, but a field larger than a
    // page is not a size anyone places on purpose.
    let size_adjustment = Adjustment::new(12.0, 4.0, 400.0, 1.0, 10.0, 0.0);
    let size = SpinButton::new(Some(&size_adjustment), 1.0, 1);
    size.set_sensitive(false);
    style_flow.append(&size);

    let color = Button::with_label("Color");
    color.set_sensitive(false);
    style_flow.append(&color);

    let style_row = ScrolledWindow::builder()
        .child(&style_flow)
        .hscrollbar_policy(PolicyType::Automatic)
        .vscrollbar_policy(PolicyType::Never)
        .propagate_natural_height(true)
        .build();

    let content = GtkBox::new(Orientation::Vertical, 10);
    content.append(&panel_heading("Form fields"));
    content.append(&place_row);
    content.append(&panel_heading("Field style"));
    content.append(&style_row);

    (
        FormFieldToolbar {
            mode,
            place,
            font,
            size,
            color,
            syncing: Rc::new(Cell::new(false)),
        },
        content,
    )
}

pub(crate) fn connect_forms_toolbar(viewer: &Viewer) {
    viewer.forms.mode.connect_toggled({
        let viewer = viewer.clone();
        move |button| super::set_mode(&viewer, button.is_active())
    });

    // At most one placement kind armed at a time — mirrors
    // `content_edit::connect_insert_toggle`'s own reasoning: the button
    // switched off as a side effect of another kind arming re-enters here
    // with `active = false`, finds `form_field_kind` has already moved on,
    // and does nothing.
    for (kind, button) in &viewer.forms.place {
        button.connect_toggled({
            let viewer = viewer.clone();
            let kind = *kind;
            move |button| {
                if button.is_active() {
                    super::set_field_kind(&viewer, Some(kind));
                    return;
                }
                let armed = viewer.state.borrow().form_field_kind;
                if armed == Some(kind) {
                    super::set_field_kind(&viewer, None);
                }
            }
        });
    }

    connect_style_controls(viewer);
}

/// Refreshes every forms control's sensitivity (and the style inspector's
/// values) from the current session — the forms twin of
/// `annotations::toolbar::update_annotation_controls`.
pub(crate) fn update_forms_controls(viewer: &Viewer) {
    let state = viewer.state.borrow();
    let Some(session) = state.session.as_ref() else {
        drop(state);
        refresh_style(viewer, None, false);
        return;
    };
    // Mirrors `command::structural_edit_refusal`: creating or modifying a
    // form field's structure needs both the annotate and the modify-contents
    // permission (ISO 32000-1 Table 22 bit 6's own text, "…and, if bit 4 is
    // also set, create or modify interactive form fields") — a button left
    // enabled on only one of the two would invite a click that
    // `structural_edit_refusal` then has to refuse anyway.
    let enabled = session.annotation_access.refusal().is_none()
        && session.content_edit_access.refusal().is_none();
    viewer.forms.mode.set_sensitive(enabled);
    for (_, button) in &viewer.forms.place {
        button.set_sensitive(enabled);
    }
    let selected_style = session
        .selected_form_field
        .and_then(|id| session.document_model.as_ref()?.form_fields.get(id))
        .map(|field| field.style);
    drop(state);
    refresh_style(viewer, selected_style, enabled);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gtk::test]
    fn gtk_ui_the_toolbar_offers_one_button_per_field_kind() {
        let (toolbar, _content) = build_forms_content();
        assert_eq!(toolbar.place.len(), FieldKind::ALL.len());
    }

    #[test]
    fn every_field_kind_has_its_own_label() {
        let mut labels: Vec<_> = FieldKind::ALL.iter().map(|kind| kind.label()).collect();
        labels.sort_unstable();
        labels.dedup();

        assert_eq!(labels.len(), FieldKind::ALL.len());
    }

    #[gtk::test]
    fn gtk_ui_forms_content_starts_with_every_control_insensitive() {
        let (toolbar, _content) = build_forms_content();

        assert!(!toolbar.mode.is_sensitive());
        for (_, button) in &toolbar.place {
            assert!(!button.is_sensitive());
        }
        assert!(!toolbar.font.is_sensitive());
        assert!(!toolbar.size.is_sensitive());
        assert!(!toolbar.color.is_sensitive());
    }
}
