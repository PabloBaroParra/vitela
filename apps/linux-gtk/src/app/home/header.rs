//! Home's header row: the brand lockup, the one search entry that filters
//! the page, and the primary "Open file" action.
//!
//! Deliberately *not* the editor toolbar. That row is about the document on
//! screen (zoom, print, find-in-page); this one is about getting to a
//! document at all, so the two live on different pages of the view stack and
//! neither has to grow controls that make no sense on the other.

use gtk::prelude::*;
use gtk::{Align, ApplicationWindow, Box as GtkBox, Button, Orientation, SearchEntry};

use crate::app::document::show_file_chooser;
use crate::app::state::Viewer;

/// Natural width of the search entry. Wide enough to read a filename back,
/// narrow enough that it does not push the open button off a small window —
/// it is a request, and the entry shrinks below it when the window does.
const SEARCH_WIDTH: i32 = 340;

pub(crate) struct HomeHeader {
    pub(crate) root: GtkBox,
    /// Filters the recents list and the tool grid. Wired by
    /// [`super::build_home`], which owns both of those.
    pub(crate) search: SearchEntry,
}

pub(crate) fn build_home_header(window: &ApplicationWindow, viewer: &Viewer) -> HomeHeader {
    // No brand lockup here. The app rail carries one directly to the left of
    // this row, and two "Vitela" marks a centimetre apart read as a bug, not
    // as branding — the rail's is the one that stays, because it is on screen
    // for the editor view too.
    let root = GtkBox::new(Orientation::Horizontal, 12);
    root.add_css_class("home-header");

    let search = SearchEntry::new();
    search.add_css_class("home-search");
    search.set_placeholder_text(Some("Search recent files and tools"));
    search.set_hexpand(true);
    search.set_halign(Align::Center);
    search.set_size_request(SEARCH_WIDTH, -1);
    // A placeholder is not an accessible name — it is announced as content,
    // not as a label, and disappears the moment anything is typed. Same
    // treatment the editor toolbar's own search entry gets.
    search.update_property(&[gtk::accessible::Property::Label(
        "Search recent files and tools",
    )]);
    root.append(&search);

    let open = Button::with_label("Open file");
    open.add_css_class("home-primary");
    open.set_valign(Align::Center);
    open.set_tooltip_text(Some("Open a PDF (Ctrl+O)"));
    open.connect_clicked({
        let window = window.clone();
        let viewer = viewer.clone();
        move |_| show_file_chooser(&window, &viewer)
    });
    root.append(&open);

    HomeHeader { root, search }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ui_tests::built_ui;

    /// The search entry is the page's filter, so `build_home` has to be handed
    /// it back — and it expands, because a fixed-width entry in a header that
    /// also holds a lockup and a button is the first thing to overflow.
    #[gtk::test]
    fn gtk_ui_the_header_exposes_an_expanding_search_entry() {
        let built = built_ui();

        let header = build_home_header(&built.window, &built.viewer);

        assert!(header.search.hexpands());
        assert_eq!(
            header.search.placeholder_text().as_deref(),
            Some("Search recent files and tools")
        );

        built.window.close();
    }
}
