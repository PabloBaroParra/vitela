//! The Home view: what the window shows before — and between — documents.
//!
//! Until this module existed the shell opened straight onto an empty page
//! canvas with the brand mark floating in it, which said nothing about what
//! the application can do or what the user opened last. Home replaces that
//! first screen with the three questions a document app is actually asked on
//! launch: *what do I open*, *what did I have open*, and *what can this do*.
//!
//! ## Shape
//!
//! Home is one page of the window's view `Stack` ([`HOME_PAGE`]); the editor —
//! toolbar plus the three-column canvas — is the other ([`EDITOR_PAGE`]). The
//! app rail and the status bar sit *outside* the stack and stay on screen for
//! both, so switching views never moves the navigation or drops a message.
//!
//! Opening a document switches to the editor (`document::show_document`);
//! the rail's Home button switches back. Neither closes anything: coming back
//! to Home leaves the open document exactly as it was.
//!
//! ## Split
//!
//! Per the repository's "no monolithic shells" rule, this module owns
//! assembly and view switching only. Each region is built by its own
//! neighbour: [`header`] (brand lockup, search, open), [`hero`] (welcome and
//! the drop zone), [`recents`] (the recently-opened documents), [`tools`]
//! (the tool grid, quick actions and the shortcut reference).

mod header;
mod hero;
pub(crate) mod recents;
pub(crate) mod tools;

use gtk::prelude::*;
use gtk::{Align, ApplicationWindow, Box as GtkBox, Orientation, ScrolledWindow};

use super::state::Viewer;

/// The view `Stack`'s child names. Shared with `build_ui` (which adds both
/// pages) and `document::show_document` (which switches to the editor when a
/// document lands), so neither has to repeat the literal.
pub(crate) const HOME_PAGE: &str = "home";
pub(crate) const EDITOR_PAGE: &str = "editor";

/// Width of Home's right-hand column. A request, not a maximum: the cards in
/// it wrap their own contents, and the left column takes every pixel this one
/// does not.
const SIDE_COLUMN_WIDTH: i32 = 268;

/// Home's own styling, installed alongside `shell::SHELL_CSS` by
/// `shell::install_shell_css`.
///
/// It lives here rather than in that constant for the same reason this
/// module exists at all: the shell's CSS describes the chrome around a
/// document, and Home is not part of that chrome. Colours are the shell
/// palette exactly — `#6b4eff` accent, `#e3e0e9` hairlines, `#625b72`
/// secondary text — so the two screens read as one application.
pub(crate) const HOME_CSS: &str = r#"
.home-view {
  background: #f8f7fb;
}

.home-header {
  background: #ffffff;
  border-bottom: 1px solid #e3e0e9;
  padding: 10px 16px;
}

.brand-word {
  color: #6b4eff;
  font-weight: 800;
  font-size: 1.15em;
}

.home-search {
  border-radius: 8px;
}

.home-primary {
  background: #6b4eff;
  color: #ffffff;
  border: 1px solid #5a3ee6;
  border-radius: 8px;
  padding: 6px 14px;
  font-weight: 700;
  transition: background-color 120ms ease;
}

.home-primary:hover,
.home-primary:focus-visible {
  background: #5a3ee6;
}

.home-body {
  padding: 24px;
}

.home-hero-title {
  font-size: 1.7em;
  font-weight: 800;
  color: #302d3a;
}

.home-hero-subtitle,
.home-empty {
  color: #625b72;
}

.home-section-title {
  font-size: 1.15em;
  font-weight: 800;
  color: #302d3a;
}

/* The drop zone is a plain box, not a button: it accepts a drag anywhere in
   its area, and the click affordance inside it is the real button. */
.home-dropzone {
  background: #f2f0f5;
  border: 2px dashed #c9c2e0;
  border-radius: 14px;
  padding: 28px;
  transition: background-color 120ms ease, border-color 120ms ease;
}

.home-dropzone.drop-active {
  background: #eee9fa;
  border-color: #6b4eff;
}

.home-card {
  background: #ffffff;
  border: 1px solid #e3e0e9;
  border-radius: 12px;
  padding: 14px;
}

.home-card-title {
  font-weight: 700;
  color: #302d3a;
}

.home-link {
  background: none;
  border: none;
  color: #6b4eff;
  font-weight: 600;
  padding: 2px 6px;
}

.home-link:hover,
.home-link:focus-visible {
  background: #eee9fa;
  border-radius: 6px;
}

.tool-tile {
  background: #f6f4fd;
  border: 1px solid #e7e2fb;
  border-radius: 10px;
  padding: 10px 6px;
  color: #51496a;
  font-weight: 600;
  transition: background-color 120ms ease;
}

.tool-tile:hover,
.tool-tile:focus-visible {
  background: #eee9fa;
}

.tool-tile:disabled {
  background: #f5f4f7;
  border-color: #eae8ef;
  color: #a49fb3;
}

.recent-card {
  background: #ffffff;
  border: 1px solid #e3e0e9;
  border-radius: 12px;
  padding: 8px;
  transition: background-color 120ms ease, border-color 120ms ease;
}

.recent-card:hover,
.recent-card:focus-visible {
  background: #faf9fe;
  border-color: #c9c2e0;
}

.recent-thumb {
  background: #e9e6ec;
  border-radius: 8px;
}

.recent-name {
  font-weight: 600;
  color: #302d3a;
}

.recent-meta {
  font-size: 0.85em;
  color: #625b72;
}

/* "Today" / "Yesterday" / "Earlier". A Label rather than a widget of its own —
   GTK4 applies padding and a radius to labels, which is the whole pill. */
.day-chip {
  background: #eee9fa;
  color: #6b4eff;
  border-radius: 999px;
  padding: 2px 10px;
  font-size: 0.82em;
  font-weight: 700;
}

/* Same reasoning as `.editor-toolbar flowboxchild` in `shell::SHELL_CSS`:
   every wrapping row on Home is a `FlowBox`, and the wrapper GTK inserts
   around each child would otherwise open gaps the card padding already
   accounts for. */
.home-view flowboxchild {
  padding: 0;
}
"#;

/// Builds the Home page.
///
/// Takes a live [`Viewer`] rather than returning controls for `build_ui` to
/// wire afterwards, the way `editor_toolbar` does: every control on this page
/// acts on the open document or opens a new one, so there is nothing to wire
/// that this module cannot wire itself once the `Viewer` exists. `build_ui`
/// therefore builds Home *after* the `Viewer`, and only adds the returned
/// widget to the stack.
pub(crate) struct Home {
    /// The page `build_ui` adds to the view stack.
    pub(crate) root: GtkBox,
    /// Kept so the app rail's Recent button can put the keyboard in the list
    /// rather than just showing the page it happens to be on.
    pub(crate) recents: recents::RecentsSection,
}

pub(crate) fn build_home(window: &ApplicationWindow, viewer: &Viewer) -> Home {
    let header = header::build_home_header(window, viewer);
    let recents = recents::build_recents_section(viewer);
    let tools_card = tools::build_tools_card(window, viewer);

    let left = GtkBox::new(Orientation::Vertical, 20);
    left.set_hexpand(true);
    left.append(&hero::build_hero(window, viewer));
    left.append(&recents.root);

    let right = GtkBox::new(Orientation::Vertical, 16);
    right.set_hexpand(false);
    right.set_valign(Align::Start);
    right.set_size_request(SIDE_COLUMN_WIDTH, -1);
    right.append(&tools_card.root);
    right.append(&tools::build_quick_actions(window, viewer));
    right.append(&tools::build_shortcuts_card());

    let body = GtkBox::new(Orientation::Horizontal, 20);
    body.add_css_class("home-body");
    body.append(&left);
    body.append(&right);

    // One entry filtering both lists, which is what "Search files and tools"
    // promises. Lowercased once here rather than in each filter.
    header.search.connect_search_changed({
        let recents = recents.clone();
        let tools_card = tools_card.clone();
        move |entry| {
            let query = entry.text().to_lowercase();
            recents.filter(&query);
            tools_card.filter(&query);
        }
    });

    // Both axes scroll, and neither policy is `Never`.
    //
    // `Never` was here first, with a comment claiming it stopped the body
    // reporting a width the window would have to satisfy. It does the
    // opposite: a `ScrolledWindow` that cannot scroll sideways has to request
    // its child's full width, and then measure that width *for the height it
    // was given*. On a window shorter than the body's 651px minimum that is a
    // width-for-height query below the child's own minimum, which GTK reports
    // as `Trying to measure GtkBox for height of 543, but it needs at least
    // 651` — and the body is left mis-measured.
    //
    // `tools_panel` and the page navigator can use `Never` because their
    // content is a wrapping `FlowBox` whose minimum width is small. Home's
    // body is two fixed columns with real minimums, so it scrolls in both
    // directions instead, and the window is free to be any size.
    let scroll = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&body)
        .build();

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("home-view");
    root.append(&header.root);
    root.append(&scroll);

    Home { root, recents }
}

pub(crate) fn show_home(viewer: &Viewer) {
    viewer.view_stack.set_visible_child_name(HOME_PAGE);
}

pub(crate) fn show_editor(viewer: &Viewer) {
    viewer.view_stack.set_visible_child_name(EDITOR_PAGE);
}

/// Applies the tool a Home tile armed before there was a document to apply it
/// to, if any, and clears it.
///
/// Called from `document::show_document`, once the freshly opened session has
/// had its controls updated — a tool whose control is insensitive for this
/// document is simply dropped by [`tools::apply`], which is why this runs
/// last rather than at the top of the open.
pub(crate) fn apply_pending_tool(viewer: &Viewer) {
    let pending = viewer.state.borrow_mut().pending_tool.take();
    if let Some(tool) = pending {
        tools::apply(viewer, tool);
    }
}

/// Whether `path` names a PDF, by extension.
///
/// Shared by the drop zone (which refuses anything else) and the recents list
/// (which filters the desktop's recent-files store down to documents this
/// application can open). Extension rather than content sniffing: both callers
/// are deciding what to *offer*, and `document::open_file` still reports a
/// real failure for a file that only looks like a PDF.
pub(super) fn is_pdf_path(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ui_tests::built_ui;
    use std::path::Path;

    /// **Home scrolls; it does not set the window's minimum size.**
    ///
    /// The body is a two-column layout with real minimums — around 677x626
    /// with nothing open — and the whole point of the `ScrolledWindow` around
    /// it is that the window never has to be that big. A `Never` scroll
    /// policy breaks exactly this: a scroller that cannot scroll on an axis
    /// has to request its child's full size on it, and then measure the other
    /// axis *for* that size. On a window shorter than the body's minimum that
    /// is a query below the child's own minimum, which GTK reports as
    /// `Trying to measure GtkBox for height of 543, but it needs at least
    /// 651`.
    ///
    /// Asserted against the body's own measurement rather than fixed numbers,
    /// so adding a card to Home cannot make this test wrong — only a policy
    /// that stops the scroller absorbing it can.
    #[gtk::test]
    fn gtk_ui_home_scrolls_instead_of_sizing_the_window_to_its_body() {
        let built = built_ui();
        let home = build_home(&built.window, &built.viewer);

        let scroll = home
            .root
            .last_child()
            .and_then(|child| child.downcast::<ScrolledWindow>().ok())
            .expect("Home's second row is the scroller holding the body");
        let body = scroll.child().expect("the scroller holds the body");

        let (body_min_height, _, _, _) = body.measure(Orientation::Vertical, -1);
        let (body_min_width, _, _, _) = body.measure(Orientation::Horizontal, -1);
        let (scroll_min_height, _, _, _) = scroll.measure(Orientation::Vertical, -1);
        let (scroll_min_width, _, _, _) = scroll.measure(Orientation::Horizontal, -1);

        assert!(
            body_min_height > 400 && body_min_width > 400,
            "this test is meaningless unless the body is genuinely large:              {body_min_width}x{body_min_height}"
        );
        assert!(
            scroll_min_height * 2 < body_min_height,
            "the scroller passes the body's {body_min_height}px height on to the window              ({scroll_min_height}px) instead of scrolling it"
        );
        assert!(
            scroll_min_width * 2 < body_min_width,
            "the scroller passes the body's {body_min_width}px width on to the window              ({scroll_min_width}px) instead of scrolling it"
        );

        built.window.close();
    }

    #[test]
    fn pdf_paths_are_recognised_whatever_the_case() {
        assert!(is_pdf_path(Path::new("/tmp/report.pdf")));
        assert!(is_pdf_path(Path::new("/tmp/report.PDF")));
        assert!(!is_pdf_path(Path::new("/tmp/report.pdf.txt")));
        assert!(!is_pdf_path(Path::new("/tmp/report")));
    }
}
