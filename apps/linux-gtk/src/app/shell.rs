//! The shell-wide CSS, gathered from every feature module into one
//! stylesheet. The navigation chrome that used to live here — the left app
//! rail — is now the top mode bar (`mode_bar`).
//!
//! **No icon-theme lookups, here or anywhere in `app/`** — no
//! `Button::from_icon_name`/`Image::from_icon_name` call exists in this
//! crate, on purpose: the app ships as an AppImage and a .deb (T-053) meant
//! to look the same regardless of which icon theme (or lack of one) the host
//! has installed, and a lookup that misses leaves a blank control with no
//! error anyone sees.
//!
//! That rule is about the *source* of an icon, not about having none. The
//! mode bar's icons come from `icons`, which ships its own SVGs inside the
//! binary and rasterises them through librsvg — the same pipeline `brand`
//! uses for the application mark. Nothing here asks the desktop for a
//! picture.

use super::content_edit::panel::EDIT_CSS;
use super::content_edit::quick_actions::QUICK_ACTIONS_CSS;
use super::home::HOME_CSS;
use super::mode_bar::MODE_BAR_CSS;
use super::organize::ORGANIZE_CSS;
use super::palette::PALETTE_CSS;

pub(crate) const SHELL_CSS: &str = r#"
.vitela-shell {
  background: @vitela_window;
  color: @vitela_text;
}

.editor-toolbar,
.status-bar {
  background: @vitela_surface;
  border-color: @vitela_border;
}

.editor-toolbar {
  border-bottom: 1px solid @vitela_border;
  padding: 10px 8px;
}

.page-navigation button,
.tools-panel button {
  min-height: 30px;
  transition: background-color 120ms ease, color 120ms ease;
}

.editor-toolbar button,
.toolbar-search button {
  min-width: 24px;
  min-height: 24px;
  padding: 5px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  box-shadow: none;
  color: @vitela_text;
  transition: background-color 120ms ease;
}

.editor-toolbar button:hover,
.toolbar-search button:hover {
  background: @vitela_hover;
}

.editor-toolbar button:checked {
  background: @vitela_checked;
  border-color: @vitela_accent_border;
}

.editor-toolbar button:focus-visible,
.toolbar-search button:focus-visible {
  outline: 2px solid @vitela_accent;
  outline-offset: -2px;
}

.editor-toolbar button:disabled,
.toolbar-search button:disabled {
  opacity: 0.38;
}

.toolbar-group {
  padding: 0 8px;
  border-right: 1px solid @vitela_divider;
}

.toolbar-group-last {
  border-right: none;
}

.editor-toolbar .page-indicator {
  min-width: 54px;
  min-height: 32px;
  padding: 0 6px;
  border: 1px solid @vitela_border_indicator;
  border-radius: 6px;
  background: @vitela_input;
}

.editor-toolbar .zoom-indicator {
  min-width: 48px;
  color: @vitela_text;
}

.toolbar-search > contents {
  padding: 14px;
  background: @vitela_surface;
  color: @vitela_text;
}

.toolbar-search entry {
  min-height: 32px;
  border: 1px solid @vitela_input_border;
  border-radius: 6px;
  background: @vitela_input;
  color: @vitela_text;
  caret-color: @vitela_text;
}

.editor-main {
  background: @vitela_recessed;
}

.navigation-panel,
.tools-panel {
  background: @vitela_surface;
  padding: 12px;
}

.navigation-panel {
  border-right: 1px solid @vitela_border;
}

.tools-panel {
  border-left: 1px solid @vitela_border;
}

.panel-heading {
  color: @vitela_text_muted;
  font-weight: 700;
  font-size: 0.85em;
}

.page-navigation button {
  color: @vitela_text_secondary;
  border-radius: 6px;
}

.page-navigation button:hover,
.page-navigation button:focus-visible {
  background: @vitela_accent_soft;
}

.canvas-frame {
  padding: 20px;
}

.canvas-frame > viewport {
  background: @vitela_canvas;
}

.status-bar {
  border-top: 1px solid @vitela_border;
  color: @vitela_text_muted;
  padding: 6px 12px;
}

.page-indicator,
.zoom-indicator {
  color: @vitela_text_secondary;
  font-weight: 600;
}

/* Every reflowing row in this shell is a `FlowBox` — the top editor toolbar
   (`editor_toolbar::build_editor_toolbar`) and the annotation/content-edit
   rows in the tools panel (`annotations::toolbar::add_annotation_toolbar`).
   `flowboxchild` is the wrapper GTK inserts around each one automatically,
   and its default padding would otherwise open uneven gaps the plain-`GtkBox`
   layouts these replaced never had. */
.editor-toolbar flowboxchild,
.tools-panel flowboxchild {
  padding: 0;
}

/* The two resizable-column drag handles (`build_ui`'s nested `Paned`s).
   GTK4's default handle is a near-invisible hairline; `wide-handle` widens
   it and this draws the grip on top so dragging it reads as an affordance
   instead of something the user has to already know is there. */
.editor-main paned > separator {
  background-color: @vitela_sash;
  min-width: 6px;
  transition: background-color 120ms ease;
}

.editor-main paned > separator:hover,
.editor-main paned > separator:selected {
  background-color: @vitela_border_strong;
}

.property-row {
  padding: 2px 0;
}

.property-key {
  color: @vitela_text_muted;
  font-weight: 600;
  font-size: 0.9em;
}

.property-value {
  color: @vitela_text;
  font-size: 0.9em;
}

.tools-placeholder {
  color: @vitela_text_muted;
  padding-top: 8px;
}

.signed-indicator {
  color: @vitela_success;
  font-weight: 700;
}
"#;

/// Every stylesheet the shell installs, in cascade order. They share one
/// palette (see `theme`) and one provider.
pub(crate) fn all_sheets() -> String {
    format!(
        "{SHELL_CSS}{MODE_BAR_CSS}{HOME_CSS}{EDIT_CSS}{QUICK_ACTIONS_CSS}{ORGANIZE_CSS}{PALETTE_CSS}"
    )
}

pub(crate) fn install_shell_css() {
    super::theme::install(all_sheets());
}
