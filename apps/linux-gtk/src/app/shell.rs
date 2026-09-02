//! The chrome around both views: the shell-wide CSS and the left app rail.
//!
//! The rail is split into two groups. The first navigates — Home, Recent,
//! My files — and is what the Home view added; the second acts on the open
//! document — Annotate, Edit, Organize pages, Sign, Protect. The rail itself
//! sits outside the window's view `Stack`, so it stays on screen for Home and
//! the editor alike.
//!
//! **No icon-theme lookups, here or anywhere in `app/`** — no
//! `Button::from_icon_name`/`Image::from_icon_name` call exists in this
//! crate, on purpose: the app ships as an AppImage and a .deb (T-053) meant
//! to look the same regardless of which icon theme (or lack of one) the host
//! has installed, and a lookup that misses leaves a blank control with no
//! error anyone sees.
//!
//! That rule is about the *source* of an icon, not about having none. The
//! rail's icons come from `icons`, which ships its own SVGs inside the
//! binary and rasterises them through librsvg — the same pipeline `brand`
//! uses for the application mark. Nothing here asks the desktop for a
//! picture.

use gtk::prelude::*;
use gtk::{
    style_context_add_provider_for_display, Box as GtkBox, Button, CssProvider, Label, Orientation,
    Separator,
};

use super::brand::build_brand_lockup;
use super::home::HOME_CSS;
use super::icons::{build_icon, Icon, MUTED_TINT, NEUTRAL_TINT};

/// Icon edge on a rail item. Sized against the label beside it, like the
/// brand lockup's mark above them both.
const RAIL_ICON_PX: i32 = 16;

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

.app-rail .brand-lockup {
  padding: 2px;
  margin-bottom: 8px;
}

/* Between the rail's navigate group and its act-on-the-document group. */
.app-rail-separator {
  background-color: #e3e0e9;
  margin: 6px 2px;
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

.signed-indicator {
  color: #1f8a4c;
  font-weight: 700;
}
"#;

/// The app rail buttons `build_ui` wires up after construction. Recent/
/// Organize pages/Protect are built and appended by [`build_app_rail`] like
/// the rest, but this shell has no feature behind them yet, so nothing
/// downstream ever needs to address them again by name — they are left out
/// of this struct rather than kept as fields no caller reads (see
/// `rail_item`'s `enabled: false` for how they end up disabled on screen).
pub(crate) struct AppRail {
    /// Switches the window's view `Stack` back to the Home page. Navigation
    /// only — the open document, if any, stays open behind it.
    pub(crate) home: Button,
    /// Home, with the keyboard already in the recents list. Its own button
    /// rather than a second `home`, because "where was I" and "what is this
    /// app" are different questions the user arrives with.
    pub(crate) recent: Button,
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

    // The real mark, not a coloured word. This is now the *only* place the
    // shell identifies itself: Home's header carried a second lockup for one
    // revision, and two "Vitela" marks a centimetre apart read as a bug. The
    // rail keeps it because the rail is on screen for both views.
    rail.append(&build_brand_lockup());

    // Navigate. `home` starts marked active because the window opens on Home.
    let home = rail_item(&rail, "Home", Icon::Home, true);
    home.add_css_class("app-rail-active");
    let recent = rail_item(&rail, "Recent", Icon::Recent, true);
    let files = rail_item(&rail, "My files", Icon::Files, true);

    let separator = Separator::new(Orientation::Horizontal);
    separator.add_css_class("app-rail-separator");
    rail.append(&separator);

    // Act on the open document.
    let annotate = rail_item(&rail, "Annotate", Icon::Annotate, true);
    let edit_pdf = rail_item(&rail, "Edit PDF", Icon::Edit, true);
    rail_item(&rail, "Organize pages", Icon::Organize, false);
    // T-186: Batch B23's signing flow (Fases 1-4) is wired end to end, so
    // this is no longer a "nothing behind it yet" section like its
    // Organize-pages/Protect neighbors.
    let sign = rail_item(&rail, "Sign", Icon::Sign, true);
    rail_item(&rail, "Protect", Icon::Protect, false);

    (
        AppRail {
            home,
            recent,
            files,
            annotate,
            edit_pdf,
            sign,
        },
        rail,
    )
}

/// Marks `active` as the current rail section and clears every other one.
///
/// The rail is not a `Stack` switcher — its items go to four different places
/// (a view page, a file chooser, a tools tab, a toggle) — so the highlight
/// has to be maintained rather than derived. One function doing it for the
/// whole rail is what stops two items from claiming to be current at once.
pub(crate) fn mark_active(rail: &GtkBox, active: &Button) {
    for item in std::iter::successors(rail.first_child(), |child| child.next_sibling())
        .filter_map(|child| child.downcast::<Button>().ok())
    {
        if &item == active {
            item.add_css_class("app-rail-active");
        } else {
            item.remove_css_class("app-rail-active");
        }
    }
}

/// Appends one nav button to `rail` and returns it. `enabled` is `false` for
/// sections this shell has no feature behind yet (Recent/Organize pages/
/// Protect) — disabled with a tooltip rather than left clickable and
/// silently doing nothing.
fn rail_item(rail: &GtkBox, label: &str, icon: Icon, enabled: bool) -> Button {
    // An icon and a left-aligned label, so the rail reads as a column of
    // destinations rather than a stack of centred buttons. The icons are
    // ours (`icons`), never the desktop's — see this module's header for why
    // that is not negotiable here.
    let tint = if enabled { NEUTRAL_TINT } else { MUTED_TINT };
    let content = GtkBox::new(Orientation::Horizontal, 8);
    content.append(&build_icon(icon, RAIL_ICON_PX, tint));
    let caption = Label::new(Some(label));
    caption.set_xalign(0.0);
    caption.set_hexpand(true);
    content.append(&caption);

    let button = Button::new();
    button.set_child(Some(&content));
    button.set_halign(gtk::Align::Fill);
    button.add_css_class("app-rail-item");
    // A `Button` given a custom child has no label of its own for the
    // accessibility layer to fall back on, so it is stated rather than
    // inferred from whichever descendant happens to hold text.
    button.update_property(&[gtk::accessible::Property::Label(label)]);
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
    // One provider for both sheets: they share a palette and a cascade, and
    // two providers at the same priority would leave which one wins a
    // question of registration order.
    provider.load_from_data(&format!("{SHELL_CSS}{HOME_CSS}"));
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

        assert_eq!(rail_label(&app_rail.sign).as_deref(), Some("Sign"));
        assert!(app_rail.sign.is_sensitive());
        assert!(app_rail.sign.tooltip_text().is_none());
    }

    /// A rail item's visible text. `rail_item` gives each button an icon and
    /// a label in a box, so `Button::label` — which only answers for the
    /// plain-`Label` child `Button::with_label` builds — returns `None`.
    fn rail_label(button: &Button) -> Option<String> {
        let content = button.child()?.downcast::<GtkBox>().ok()?;
        std::iter::successors(content.first_child(), |child| child.next_sibling())
            .filter_map(|child| child.downcast::<Label>().ok())
            .map(|label| label.text().to_string())
            .next()
    }

    fn rail_button(rail: &GtkBox, label: &str) -> Button {
        std::iter::successors(rail.first_child(), |child| child.next_sibling())
            .filter_map(|child| child.downcast::<Button>().ok())
            .find(|button| rail_label(button).as_deref() == Some(label))
            .unwrap_or_else(|| panic!("the rail must offer a {label} button"))
    }

    /// Sections still without a feature behind them keep the disabled
    /// treatment `rail_item` gives every `enabled: false` entry.
    #[gtk::test]
    fn gtk_ui_sections_without_a_feature_stay_disabled() {
        let (_app_rail, rail_box) = build_app_rail();

        let organize = rail_button(&rail_box, "Organize pages");

        assert!(!organize.is_sensitive());
        assert_eq!(
            organize.tooltip_text().as_deref(),
            Some("Not available yet")
        );
    }

    /// Recent's own regression lock, the twin of the Sign one above: the Home
    /// view gave it a feature, so a silent revert to `rail_item(.., false)`
    /// must fail here rather than only in manual QA.
    #[gtk::test]
    fn gtk_ui_the_recent_rail_button_is_enabled() {
        let (app_rail, _rail_box) = build_app_rail();

        assert!(app_rail.recent.is_sensitive());
        assert!(app_rail.recent.tooltip_text().is_none());
    }

    /// The window opens on Home, so Home is the section marked current before
    /// anything is clicked — and exactly one section is ever marked.
    #[gtk::test]
    fn gtk_ui_the_rail_marks_one_active_section_starting_with_home() {
        let (app_rail, rail_box) = build_app_rail();

        assert!(app_rail.home.has_css_class("app-rail-active"));

        mark_active(&rail_box, &app_rail.annotate);

        assert!(app_rail.annotate.has_css_class("app-rail-active"));
        assert!(!app_rail.home.has_css_class("app-rail-active"));
        let marked = std::iter::successors(rail_box.first_child(), |child| child.next_sibling())
            .filter_map(|child| child.downcast::<Button>().ok())
            .filter(|button| button.has_css_class("app-rail-active"))
            .count();
        assert_eq!(marked, 1);
    }
}
