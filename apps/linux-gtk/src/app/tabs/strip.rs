//! The tab strip: one tab per open document above the editor and Organize
//! screens — its name, an unsaved marker and a close button.
//!
//! Built by hand from boxes and buttons: the shell links plain GTK 4, and
//! libadwaita's tab bar is not one of its dependencies. The strip is redrawn
//! from a list of [`TabView`]s and holds no state of its own beyond the list
//! it last drew, which is what lets a redraw that would change nothing skip
//! rebuilding the widgets.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Label, Orientation, PolicyType, ScrolledWindow};

use super::{close::close_tab, request_activation, TabId};
use crate::app::i18n::{tr, tr_args};
use crate::app::state::Viewer;

pub(crate) const TABS_CSS: &str = r#"
.document-tabs {
  background: @vitela_surface;
  border-bottom: 1px solid @vitela_border;
  padding: 4px 8px 0 8px;
}

.document-tab {
  border: 1px solid transparent;
  border-bottom: none;
  border-radius: 6px 6px 0 0;
}

.document-tab.active {
  background: @vitela_window;
  border-color: @vitela_border;
}

.document-tab button {
  min-height: 24px;
  padding: 2px 6px;
  border: none;
  background: transparent;
  box-shadow: none;
  color: @vitela_text;
}

.document-tab .tab-unsaved {
  color: @vitela_accent;
}
"#;

/// The marker in front of a tab's name while its document has unsaved
/// changes. A symbol, not a word — the accessible name says it in words.
const UNSAVED_MARKER: &str = "\u{2022}";
/// The close button's face: a symbol rather than an icon-theme lookup,
/// which this crate never makes (see `shell`).
const CLOSE_MARKER: &str = "\u{00d7}";

/// What one tab shows.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TabView {
    pub(super) id: TabId,
    pub(super) name: String,
    pub(super) path: Option<PathBuf>,
    pub(super) dirty: bool,
    pub(super) active: bool,
}

#[derive(Clone)]
pub(crate) struct TabStrip {
    pub(crate) root: ScrolledWindow,
    tabs: GtkBox,
    /// What the strip last drew.
    shown: Rc<RefCell<Vec<TabView>>>,
}

pub(crate) fn build_strip() -> TabStrip {
    let tabs = GtkBox::new(Orientation::Horizontal, 2);
    let root = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Automatic)
        .vscrollbar_policy(PolicyType::Never)
        .propagate_natural_height(true)
        .child(&tabs)
        .build();
    root.add_css_class("document-tabs");
    root.update_property(&[gtk::accessible::Property::Label(&tr("tabs-strip"))]);
    root.set_visible(false);
    TabStrip {
        root,
        tabs,
        shown: Rc::default(),
    }
}

/// Redraws the strip as `views`, unless that is what it already shows.
pub(super) fn render(viewer: &Viewer, views: &[TabView]) {
    let strip = &viewer.tab_strip;
    if strip.shown.borrow().as_slice() == views {
        return;
    }
    while let Some(child) = strip.tabs.first_child() {
        strip.tabs.remove(&child);
    }
    for view in views {
        strip.tabs.append(&build_tab(viewer, view));
    }
    *strip.shown.borrow_mut() = views.to_vec();
}

fn build_tab(viewer: &Viewer, view: &TabView) -> GtkBox {
    let tab = GtkBox::new(Orientation::Horizontal, 0);
    tab.add_css_class("document-tab");
    if view.active {
        tab.add_css_class("active");
    }

    let face = GtkBox::new(Orientation::Horizontal, 4);
    let marker = Label::new(Some(UNSAVED_MARKER));
    marker.add_css_class("tab-unsaved");
    marker.set_visible(view.dirty);
    let name = Label::new(Some(&view.name));
    name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    name.set_max_width_chars(28);
    face.append(&marker);
    face.append(&name);

    let select = Button::new();
    select.set_child(Some(&face));
    let tooltip = view
        .path
        .as_ref()
        .map_or_else(|| view.name.clone(), |path| path.display().to_string());
    select.set_tooltip_text(Some(&tooltip));
    let accessible = if view.dirty {
        tr_args("tabs-tab-unsaved", &[("name", view.name.clone().into())])
    } else {
        view.name.clone()
    };
    select.update_property(&[gtk::accessible::Property::Label(&accessible)]);
    select.connect_clicked({
        let viewer = viewer.clone();
        let id = view.id;
        move |_| request_activation(&viewer, id)
    });

    let close = Button::with_label(CLOSE_MARKER);
    close.set_tooltip_text(Some(&tr("tabs-close-tooltip")));
    close.update_property(&[gtk::accessible::Property::Label(&tr_args(
        "tabs-close-named",
        &[("name", view.name.clone().into())],
    ))]);
    close.connect_clicked({
        let viewer = viewer.clone();
        let id = view.id;
        move |_| {
            if let Some(window) = viewer.window() {
                close_tab(&window, &viewer, id);
            }
        }
    });

    tab.append(&select);
    tab.append(&close);
    tab
}

/// Test access to the drawn tabs: `(name, unsaved marker shown, active)`
/// per tab, plus the buttons that select and close it.
#[cfg(test)]
pub(super) struct DrawnTab {
    pub(super) name: String,
    pub(super) unsaved: bool,
    pub(super) active: bool,
    pub(super) tooltip: Option<String>,
    pub(super) select: Button,
    pub(super) close: Button,
}

#[cfg(test)]
pub(super) fn drawn(strip: &TabStrip) -> Vec<DrawnTab> {
    let mut drawn = Vec::new();
    let mut child = strip.tabs.first_child();
    while let Some(tab) = child {
        child = tab.next_sibling();
        let select = tab
            .first_child()
            .and_downcast::<Button>()
            .expect("a tab starts with its select button");
        let close = select
            .next_sibling()
            .and_downcast::<Button>()
            .expect("and ends with its close button");
        let face = select.child().expect("the select button's face");
        let marker = face.first_child().and_downcast::<Label>().expect("marker");
        let name = marker.next_sibling().and_downcast::<Label>().expect("name");
        drawn.push(DrawnTab {
            name: name.text().to_string(),
            unsaved: marker.is_visible(),
            active: tab.has_css_class("active"),
            tooltip: select.tooltip_text().map(|text| text.to_string()),
            select,
            close,
        });
    }
    drawn
}
