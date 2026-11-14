//! Bold, italic and underline for existing text: three toggles on the Edit
//! page's text card, and Ctrl+B / Ctrl+I / Ctrl+U.
//!
//! They act on the text content-edit mode is pointed at — the open editor's
//! run or block, else the selected block, else every run of a
//! multi-selection — and record one `Command::RestyleTextRun` per run that
//! changes, a batch when there are several, through the same queue every
//! content edit takes (`command::record`). How the style reaches the page
//! (a sibling face, a stroked outline, a slant, a line) is the core's call:
//! see `pdf_edit::restyle`.
//!
//! With text selected in the open editor, only the runs the selection
//! covers whole are restyled (`selection`): the core restyles whole runs,
//! so a selection inside a run is refused rather than rounded out. A caret
//! alone, or the whole text selected, restyles every run.
//!
//! While the editor is open a press keeps it open — the buttons refuse the
//! focus a click would give them, like "Delete text". When every run was
//! restyled the editor is redrawn in the new weight and slant, so the reader
//! sees what they typed in the face it will be saved in. When only some
//! were, it keeps its face: the camouflage draws the whole field in one face
//! (a stylesheet, not a per-character tag), and slanting all of it would
//! show a restyle that is not the one recorded.
//!
//! A bold (or italic) that is the run's own font, with no regular sibling on
//! the page, cannot be taken off: the button stays pressed and insensitive,
//! with a tooltip saying why, rather than faking a regular face.

mod model;
mod reselect;
mod selection;

use gtk::prelude::*;
use gtk::{gio, Box as GtkBox, Label, Orientation, ToggleButton};
use pdf_document::TextRun;

use crate::app::annotations;
use crate::app::i18n::tr;
use crate::app::shortcuts::{self, Shortcut};
use crate::app::state::{DocumentSession, Viewer};
use crate::app::write::refresh_preview;

use super::{command, editor};
pub(crate) use model::Style;
pub(crate) use reselect::restore as restore_selection;

/// The three buttons. Held on the `Viewer`; every field is a widget handle.
#[derive(Clone)]
pub(crate) struct StyleToggles {
    pub(crate) bold: ToggleButton,
    pub(crate) italic: ToggleButton,
    pub(crate) underline: ToggleButton,
    /// The selected block to select again once a toggle's refresh lands.
    reselect: reselect::Pending,
}

impl StyleToggles {
    fn button(&self, style: Style) -> &ToggleButton {
        match style {
            Style::Bold => &self.bold,
            Style::Italic => &self.italic,
            Style::Underline => &self.underline,
        }
    }
}

const STYLES: [Style; 3] = [Style::Bold, Style::Italic, Style::Underline];

fn shortcut(style: Style) -> Shortcut {
    match style {
        Style::Bold => shortcuts::BOLD,
        Style::Italic => shortcuts::ITALIC,
        Style::Underline => shortcuts::UNDERLINE,
    }
}

fn tooltip_key(style: Style) -> &'static str {
    match style {
        Style::Bold => "edit-style-bold-tooltip",
        Style::Italic => "edit-style-italic-tooltip",
        Style::Underline => "edit-style-underline-tooltip",
    }
}

/// Builds the three toggles, insensitive, in a row of their own.
///
/// Each shows its letter in its own style — no icon of the shell's says
/// "bold", and the icon theme is off limits (`quick_actions`' cut and copy
/// are words for the same reason).
pub(crate) fn build() -> (StyleToggles, GtkBox) {
    let row = GtkBox::new(Orientation::Horizontal, 4);
    row.add_css_class("edit-style-toggles");
    let make = |markup: &str, name: &str, tooltip: &str| {
        let label = Label::new(None);
        label.set_markup(markup);
        let button = ToggleButton::new();
        button.set_child(Some(&label));
        button.add_css_class("edit-tile");
        button.set_tooltip_text(Some(tooltip));
        button.update_property(&[gtk::accessible::Property::Label(name)]);
        // A click must not take the focus from an open editor: it commits on
        // focus-out, and the press would close the box it restyles.
        button.set_focus_on_click(false);
        button.set_sensitive(false);
        row.append(&button);
        button
    };
    let toggles = StyleToggles {
        bold: make(
            "<b>B</b>",
            &tr("edit-style-bold"),
            &tr("edit-style-bold-tooltip"),
        ),
        italic: make(
            "<i>I</i>",
            &tr("edit-style-italic"),
            &tr("edit-style-italic-tooltip"),
        ),
        underline: make(
            "<u>U</u>",
            &tr("edit-style-underline"),
            &tr("edit-style-underline-tooltip"),
        ),
        reselect: reselect::Pending::default(),
    };
    (toggles, row)
}

/// Wires the buttons and registers the three window shortcuts.
pub(crate) fn connect(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
) {
    for style in STYLES {
        viewer.style_toggles.button(style).connect_clicked({
            let viewer = viewer.clone();
            move |_| toggle(&viewer, style)
        });
        let shortcut = shortcut(style);
        let action = gio::SimpleAction::new(shortcut.action, None);
        action.set_enabled(false);
        action.connect_activate({
            let viewer = viewer.clone();
            move |_, _| toggle(&viewer, style)
        });
        window.add_action(&action);
        shortcut.install(application);
    }
    // Whether a key may fire follows the focus, as for F2 and Delete.
    window.connect_focus_widget_notify({
        let viewer = viewer.clone();
        move |_| update(&viewer)
    });
}

/// The runs a toggle acts on, and where they come from.
struct Target {
    runs: Vec<TextRun>,
    scope: Scope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// A selected block or a multi-selection; no editor is open.
    Selected,
    /// Every run of the open editor.
    Editor,
    /// Some of the open editor's runs: the ones its selection covers.
    EditorSelection,
    /// The open editor's selection covers no run whole. `runs` are all of
    /// the editor's, for the buttons to show; a press is refused.
    NoWholeRun,
}

/// The runs a toggle acts on: the open editor's (those its selection
/// covers, when text is selected), else the selected block's, else a
/// multi-selection's. An editor composing an insertion has none.
fn target_runs(session: &DocumentSession) -> Option<Target> {
    if let Some(editor) = session.content_editor.as_ref() {
        if editor.is_insertion {
            return None;
        }
        let runs = runs_of(&editor.run, &editor.block);
        let texts: Vec<&str> = runs.iter().map(|run| run.text.as_str()).collect();
        let selected =
            selection::selected_runs(&editor.field.text(), &texts, editor.field.selection());
        return Some(match selected {
            selection::Selected::Whole => Target {
                runs,
                scope: Scope::Editor,
            },
            selection::Selected::Runs(indices) => Target {
                runs: indices
                    .into_iter()
                    .map(|index| runs[index].clone())
                    .collect(),
                scope: Scope::EditorSelection,
            },
            selection::Selected::NoWholeRun => Target {
                runs,
                scope: Scope::NoWholeRun,
            },
        });
    }
    if let Some(selected) = session.selected_text_block.as_ref() {
        return Some(Target {
            runs: runs_of(&selected.run, &selected.block),
            scope: Scope::Selected,
        });
    }
    let runs = &session.content_selection.as_ref()?.members.runs;
    (!runs.is_empty()).then(|| Target {
        runs: runs.clone(),
        scope: Scope::Selected,
    })
}

/// A block's runs, or the run standing alone.
fn runs_of(run: &TextRun, block: &[TextRun]) -> Vec<TextRun> {
    if block.is_empty() {
        vec![run.clone()]
    } else {
        block.to_vec()
    }
}

/// Shows the target's state on the buttons, and enables the shortcuts while
/// there is one. A text field other than the content editor keeps the keys.
pub(crate) fn update(viewer: &Viewer) {
    let (toggles, editing) = {
        let state = viewer.state.borrow();
        let found = state.session.as_ref().and_then(|session| {
            if !state.content_edit_mode || session.content_edit_access.refusal().is_some() {
                return None;
            }
            let target = target_runs(session)?;
            // The first targeted run only: what the buttons show is its
            // state, and a read parses the page, which this runs on every
            // selection change. A press reads them all (`record`).
            let states = model::run_states(session, &target.runs[..1])?;
            Some((model::toggles(&states)?, target.scope != Scope::Selected))
        });
        match found {
            Some((toggles, editing)) => (Some(toggles), editing),
            None => (None, false),
        }
    };
    let keys = toggles.is_some() && (editing || !annotations::text_input_has_focus(viewer));
    for style in STYLES {
        let button = viewer.style_toggles.button(style);
        let (active, removable) = match (toggles, style) {
            (None, _) => (false, true),
            (Some(toggles), Style::Bold) => (toggles.look.bold, toggles.bold_removable),
            (Some(toggles), Style::Italic) => (toggles.look.italic, toggles.italic_removable),
            (Some(toggles), Style::Underline) => (toggles.look.underline, true),
        };
        button.set_active(active);
        button.set_sensitive(toggles.is_some() && removable);
        button.set_tooltip_text(Some(&tr(if removable {
            tooltip_key(style)
        } else if style == Style::Bold {
            model::BOLD_FIXED
        } else {
            model::ITALIC_FIXED
        })));
        if let Some(action) = viewer
            .window()
            .and_then(|window| window.lookup_action(shortcut(style).action))
            .and_then(|action| action.downcast::<gio::SimpleAction>().ok())
        {
            action.set_enabled(keys);
        }
    }
}

/// Turns `style` on or off for the target, recording the restyle; the open
/// editor, if any, stays open, and is redrawn in the new face when every
/// one of its runs was restyled.
pub(crate) fn toggle(viewer: &Viewer, style: Style) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(&refusal);
        update(viewer);
        return;
    }
    let outcome = {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        record(session, style)
    };
    match outcome {
        Ok(Some(restyled)) => {
            // The refresh drops the selected block; it is selected again
            // when the refresh lands, so the next toggle stacks on this one.
            if let Some(session) = viewer.state.borrow().session.as_ref() {
                viewer.style_toggles.reselect.remember(session);
            }
            if let Some((provider, css)) = restyled {
                provider.load_from_data(&css);
                editor::refit(viewer);
            }
            refresh_preview(viewer, tr("edit-text-restyled"));
        }
        Ok(None) => {}
        Err(error) => viewer.status.set_text(&error),
    }
    update(viewer);
}

/// The CSS that redraws an open editor after a restyle, when its face
/// changed.
type EditorRestyle = Option<(gtk::CssProvider, String)>;

/// Records the press on `session`: `Ok(None)` when there was nothing to do.
fn record(session: &mut DocumentSession, style: Style) -> Result<Option<EditorRestyle>, String> {
    let Some(Target { runs, scope }) = target_runs(session) else {
        return Ok(None);
    };
    if scope == Scope::NoWholeRun {
        return Err(tr("edit-style-select-whole-words"));
    }
    let states = model::run_states(session, &runs).ok_or_else(|| tr("edit-style-unavailable"))?;
    let Some(command) = model::command(&states, style).map_err(tr)? else {
        return Ok(None);
    };
    let face = model::pressed(&states, style);
    if matches!(command, pdf_document::Command::Batch(_)) {
        command::record_batch(session, Some(command))?;
    } else {
        command::record(session, command)?;
    }
    // Only a restyle of every run changes the face the editor shows.
    let restyle = (scope == Scope::Editor)
        .then(|| {
            session
                .content_editor
                .as_mut()?
                .camouflage
                .as_mut()?
                .restyle_face(face.bold, face.italic)
        })
        .flatten();
    Ok(Some(restyle))
}

#[cfg(test)]
mod tests;
