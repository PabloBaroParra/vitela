//! The chrome around the page canvas: the shell-wide CSS and the left app
//! rail (Files / Recent / Annotate / Edit PDF / Organize pages / Sign /
//! Protect).
//!
//! Text-only by design, like every other control in this shell — no
//! `Button::from_icon_name`/`Image::from_icon_name` calls exist anywhere in
//! `app/`, on purpose: the app ships as an AppImage and a .deb (T-053) meant
//! to look the same regardless of which icon theme (or lack of one) the host
//! has installed, so nothing here depends on icon-theme lookups succeeding.

use gtk::prelude::*;
use gtk::{
    style_context_add_provider_for_display, Box as GtkBox, Button, CssProvider, Label, Orientation,
};

pub(crate) const SHELL_CSS: &str = r#"
.vitela-shell {
  background: #f8f7fb;
  color: #302d3a;
}

.editor-toolbar,
.status-bar {
  background: #ffffff;
  border-color: #e3e0e9;
}

.editor-toolbar {
  border-bottom: 1px solid #e3e0e9;
  padding: 8px 12px;
}

.editor-toolbar button,
.page-navigation button,
.tools-panel button,
.app-rail-item {
  min-height: 30px;
  transition: background-color 120ms ease, color 120ms ease;
}

.editor-toolbar button:hover,
.editor-toolbar button:focus-visible {
  background: #f2f0f5;
}

.editor-main {
  background: #f2f0f5;
}

.app-rail,
.navigation-panel,
.tools-panel {
  background: #ffffff;
  padding: 12px;
}

.app-rail {
  border-right: 1px solid #e3e0e9;
}

.app-rail-brand {
  color: #6b4eff;
  font-weight: 800;
  font-size: 1.05em;
}

.app-rail-item {
  color: #51496a;
  border-radius: 6px;
}

.app-rail-item:hover,
.app-rail-item:focus-visible {
  background: #eee9fa;
}

.app-rail-item.app-rail-active {
  background: #eee9fa;
  color: #6b4eff;
  font-weight: 700;
}

.navigation-panel {
  border-right: 1px solid #e3e0e9;
}

.tools-panel {
  border-left: 1px solid #e3e0e9;
}

.panel-heading {
  color: #625b72;
  font-weight: 700;
  font-size: 0.85em;
}

.page-navigation button {
  color: #51496a;
  border-radius: 6px;
}

.page-navigation button:hover,
.page-navigation button:focus-visible {
  background: #eee9fa;
}

.canvas-frame {
  padding: 20px;
}

.canvas-frame > viewport {
  background: #e9e6ec;
}

.status-bar {
  border-top: 1px solid #e3e0e9;
  color: #625b72;
  padding: 6px 12px;
}

.page-indicator,
.zoom-indicator {
  color: #51496a;
  font-weight: 600;
}

.tools-tab-switcher {
  border-bottom: 1px solid #e3e0e9;
  padding-bottom: 8px;
}

.tools-tab-switcher button {
  min-height: 26px;
}

.tools-tab-switcher button:hover,
.tools-tab-switcher button:focus-visible {
  background: #eee9fa;
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
  background-color: #e9e6ec;
  min-width: 6px;
  transition: background-color 120ms ease;
}

.editor-main paned > separator:hover,
.editor-main paned > separator:selected {
  background-color: #c9c2e0;
}

.property-row {
  padding: 2px 0;
}

.property-key {
  color: #625b72;
  font-weight: 600;
  font-size: 0.9em;
}

.property-value {
  color: #302d3a;
  font-size: 0.9em;
}

.tools-placeholder {
  color: #625b72;
  padding-top: 8px;
}
"#;

/// The app rail buttons `build_ui` wires up after construction. Recent/
/// Organize pages/Protect are built and appended by [`build_app_rail`] like
/// the rest, but this shell has no feature behind them yet, so nothing
/// downstream ever needs to address them again by name — they are left out
/// of this struct rather than kept as fields no caller reads (see
/// `rail_item`'s `enabled: false` for how they end up disabled on screen).
pub(crate) struct AppRail {
    /// Wired to `win.open`, set directly on the widget — the same command as
    /// the toolbar's Open PDF button, just reachable from the rail.
    pub(crate) files: Button,
    /// Wired by the caller once a [`super::state::Viewer`] exists — see
    /// `build_ui`.
    pub(crate) annotate: Button,
    /// Wired by the caller once a [`super::state::Viewer`] exists — see
    /// `build_ui`.
    pub(crate) edit_pdf: Button,
    /// Wired by the caller once a [`super::state::Viewer`] exists — see
    /// `build_ui`. Batch B23 Fase 5 (T-186): switches the tools panel to its
    /// "Fill & Sign" page, the same navigation gesture `annotate`/`edit_pdf`
    /// already perform for their own pages.
    pub(crate) sign: Button,
}

/// Builds the rail widget. Callers wire `files` to `win.open` themselves
/// (a plain `set_action_name`, no closure needed) and connect `annotate`/
/// `edit_pdf`/`sign` once a `Viewer` exists to open onto — see `build_ui`.
pub(crate) fn build_app_rail() -> (AppRail, GtkBox) {
    let rail = GtkBox::new(Orientation::Vertical, 4);
    rail.add_css_class("app-rail");
    // Icon-rail width, always — never a pane a resize should be able to
    // grow, unlike its neighbors in `build_ui`'s two `Paned`s.
    rail.set_hexpand(false);
    rail.set_width_request(112);
    rail.update_property(&[gtk::accessible::Property::Label("Vitela sections")]);

    let brand = Label::new(Some("Vitela"));
    brand.set_xalign(0.0);
    brand.add_css_class("app-rail-brand");
    brand.set_margin_bottom(8);
    rail.append(&brand);

    let files = rail_item(&rail, "Files", true);
    files.add_css_class("app-rail-active");
    rail_item(&rail, "Recent", false);
    let annotate = rail_item(&rail, "Annotate", true);
    let edit_pdf = rail_item(&rail, "Edit PDF", true);
    rail_item(&rail, "Organize pages", false);
    // T-186: Batch B23's signing flow (Fases 1-4) is wired end to end, so
    // this is no longer a "nothing behind it yet" section like its
    // Organize-pages/Protect neighbors.
    let sign = rail_item(&rail, "Sign", true);
    rail_item(&rail, "Protect", false);

    (
        AppRail {
            files,
            annotate,
            edit_pdf,
            sign,
        },
        rail,
    )
}

/// Appends one nav button to `rail` and returns it. `enabled` is `false` for
/// sections this shell has no feature behind yet (Recent/Organize pages/
/// Protect) — disabled with a tooltip rather than left clickable and
/// silently doing nothing.
fn rail_item(rail: &GtkBox, label: &str, enabled: bool) -> Button {
    let button = Button::with_label(label);
    button.set_halign(gtk::Align::Fill);
    // `Button::with_label` builds its child as a plain `Label`; reaching in to
    // left-align it is the same trick `Button::label()`/`set_label()` use
    // under the hood, not a private-API workaround.
    if let Some(child_label) = button
        .child()
        .and_then(|child| child.downcast::<Label>().ok())
    {
        child_label.set_xalign(0.0);
    }
    button.add_css_class("app-rail-item");
    if !enabled {
        button.set_sensitive(false);
        button.set_tooltip_text(Some("Not available yet"));
    }
    rail.append(&button);
    button
}

pub(crate) fn install_shell_css() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let provider = CssProvider::new();
    provider.load_from_data(SHELL_CSS);
    style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-186's own regression lock: `rail_item(&rail, "Sign", false)` is what
    /// this batch changes to `true` — a silent revert back to `false` would
    /// otherwise only show up as a manual-QA finding, not a test failure.
    #[gtk::test]
    fn gtk_ui_the_sign_rail_button_is_enabled() {
        let (app_rail, _rail_box) = build_app_rail();

        assert_eq!(app_rail.sign.label().as_deref(), Some("Sign"));
        assert!(app_rail.sign.is_sensitive());
        assert!(app_rail.sign.tooltip_text().is_none());
    }

    /// Sections still without a feature behind them keep the disabled
    /// treatment `rail_item` gives every `enabled: false` entry.
    #[gtk::test]
    fn gtk_ui_sections_without_a_feature_stay_disabled() {
        let (_app_rail, rail_box) = build_app_rail();

        let recent = std::iter::successors(rail_box.first_child(), |child| child.next_sibling())
            .filter_map(|child| child.downcast::<Button>().ok())
            .find(|button| button.label().as_deref() == Some("Recent"))
            .expect("the rail must still offer a Recent button");

        assert!(!recent.is_sensitive());
        assert_eq!(recent.tooltip_text().as_deref(), Some("Not available yet"));
    }
}
