//! Home's right-hand column: the tool grid, the quick actions, and the
//! keyboard reference.
//!
//! ## What a tool tile does
//!
//! A tile is a *destination*, not a command: it takes the user to the control
//! that does the work, in the editor. With a document open it switches to the
//! editor and arms or reveals that control. With no document open it arms the
//! tool as [`crate::app::state::ViewerState::pending_tool`] and opens the file
//! chooser, so "Sign" from a cold start is one gesture — pick a file, land on
//! Fill & Sign — rather than open, hunt, click.
//!
//! [`apply`] is the single definition of what each tool *means*; the app rail
//! calls it too, so the rail and the grid cannot come to disagree about what
//! "Edit" opens.
//!
//! ## The third card
//!
//! The reference design carries a storage-quota card here. This shell has no
//! account and no cloud, so the honest version of that card would be a
//! progress bar over invented numbers. The slot holds the shortcut reference
//! instead: real, useful on the screen the user reads before starting, and it
//! keeps the column's three-card rhythm.

use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    Align, ApplicationWindow, Box as GtkBox, Button, FlowBox, Label, Orientation, SelectionMode,
};

use crate::app::document::{new_blank_document, open_sample, show_file_chooser, SampleKind};
use crate::app::icons::{
    build_icon, Icon, ACCENT_TINT, ANNOTATE_TINT, COMPRESS_TINT, EDIT_TINT, MUTED_TINT,
    ORGANIZE_TINT, PROTECT_TINT, SIGN_TINT,
};
use crate::app::state::{HomeTool, Viewer};
use crate::app::tools_panel::{property_row, FILL_SIGN_PAGE};

/// One tile: its label, the tool it opens (`None` for a section this shell
/// has no feature behind yet), its icon and accent, and what it is for.
///
/// The remaining `None` entries are kept visible and disabled rather than dropped,
/// the same treatment `shell::rail_item` gives its own unfinished sections —
/// a disabled control with a tooltip says "later", an absent one says
/// "never", and only one of those is true. Their accent is carried here all
/// the same: [`tile_tint`] decides whether a tile is coloured, so the palette
/// stays one table rather than a colour in one place and an exception in
/// another.
struct ToolTile {
    label: &'static str,
    tool: Option<HomeTool>,
    icon: Icon,
    tint: &'static str,
    description: &'static str,
}

/// The grid, in reading order.
const TOOLS: [ToolTile; 6] = [
    ToolTile {
        label: "Edit",
        tool: Some(HomeTool::Edit),
        icon: Icon::Edit,
        tint: EDIT_TINT,
        description: "Retype text and replace images",
    },
    ToolTile {
        label: "Annotate",
        tool: Some(HomeTool::Annotate),
        icon: Icon::Annotate,
        tint: ANNOTATE_TINT,
        description: "Highlight, draw, and add notes",
    },
    ToolTile {
        label: "Sign",
        tool: Some(HomeTool::Sign),
        icon: Icon::Sign,
        tint: SIGN_TINT,
        description: "Sign with a certificate, card, or token",
    },
    ToolTile {
        label: "Organize",
        tool: Some(HomeTool::Organize),
        icon: Icon::Organize,
        tint: ORGANIZE_TINT,
        description: "Reorder and delete pages",
    },
    ToolTile {
        label: "Compress",
        tool: None,
        icon: Icon::Compress,
        tint: COMPRESS_TINT,
        description: "",
    },
    ToolTile {
        label: "Protect",
        tool: None,
        icon: Icon::Protect,
        tint: PROTECT_TINT,
        description: "",
    },
];

/// How many tiles fit across the right-hand column.
const TOOLS_PER_ROW: u32 = 3;

/// Icon edge on a tool tile, and on a quick-action row. The tile is a target
/// you aim at, the row is a line you read — so the tile's icon leads above
/// its label and the row's sits at the height of its own text.
const TILE_ICON_PX: i32 = 24;
const ROW_ICON_PX: i32 = 16;

#[derive(Clone)]
pub(crate) struct ToolsCard {
    pub(crate) root: GtkBox,
    /// Each tile with its lowercased label, for the header's filter.
    tiles: Rc<Vec<(String, Button)>>,
}

pub(crate) fn build_tools_card(window: &ApplicationWindow, viewer: &Viewer) -> ToolsCard {
    let grid = FlowBox::new();
    grid.set_selection_mode(SelectionMode::None);
    grid.set_homogeneous(true);
    grid.set_row_spacing(8);
    grid.set_column_spacing(8);
    grid.set_max_children_per_line(TOOLS_PER_ROW);
    grid.set_min_children_per_line(TOOLS_PER_ROW);

    let tiles = TOOLS
        .iter()
        .map(|entry| {
            let content = GtkBox::new(Orientation::Vertical, 6);
            content.set_halign(Align::Center);
            content.append(&build_icon(entry.icon, TILE_ICON_PX, tile_tint(entry)));
            content.append(&Label::new(Some(entry.label)));

            let tile = Button::new();
            tile.set_child(Some(&content));
            tile.add_css_class("tool-tile");
            // Set explicitly: a `Button` given a custom child no longer has a
            // label of its own for the accessibility layer to fall back on.
            tile.update_property(&[gtk::accessible::Property::Label(entry.label)]);
            match entry.tool {
                Some(tool) => {
                    tile.set_tooltip_text(Some(entry.description));
                    tile.connect_clicked({
                        let window = window.clone();
                        let viewer = viewer.clone();
                        move |_| open_tool(&window, &viewer, tool)
                    });
                }
                None => {
                    tile.set_sensitive(false);
                    tile.set_tooltip_text(Some("Not available yet"));
                }
            }
            grid.append(&tile);
            (entry.label.to_lowercase(), tile)
        })
        .collect();

    let root = card("Tools");
    root.append(&grid);

    ToolsCard {
        root,
        tiles: Rc::new(tiles),
    }
}

/// A tile's icon colour: its own accent when the tool is live, the muted grey
/// when it is not.
///
/// The reference design colours all five of its tools, but all five are live
/// in that drawing. Three of ours are not, and a full-strength brand colour
/// inside a greyed-out tile is the one signal on the card that says "click
/// me" — which is exactly what the disabled state exists to deny.
fn tile_tint(entry: &ToolTile) -> &'static str {
    if entry.tool.is_some() {
        entry.tint
    } else {
        MUTED_TINT
    }
}

impl ToolsCard {
    pub(crate) fn filter(&self, query: &str) {
        let mut shown = 0;
        for (label, tile) in self.tiles.iter() {
            let visible = label.contains(query);
            tile.set_visible(visible);
            shown += usize::from(visible);
        }
        // A card with a title over an empty grid reads as broken rather than
        // as filtered, so the whole card leaves when nothing in it matches.
        self.root.set_visible(shown > 0);
    }
}

/// Opens `tool`, picking a document first if there is not one already.
///
/// `pub(crate)` because the app rail's Annotate/Edit/Sign buttons are the
/// same gesture from a different place: before this existed each of them
/// carried its own copy of "focus this control, unless it is insensitive",
/// and the rail's copy had already drifted — it did nothing at all with no
/// document open.
pub(crate) fn open_tool(window: &ApplicationWindow, viewer: &Viewer, tool: HomeTool) {
    if viewer.state.borrow().session.is_some() {
        super::show_editor(viewer);
        apply(viewer, tool);
        return;
    }
    viewer.state.borrow_mut().pending_tool = Some(tool);
    show_file_chooser(window, viewer);
}

/// Reveals the control behind `tool` in the editor.
///
/// The one definition of what each tool means, shared by the Home grid, the
/// app rail, and [`super::apply_pending_tool`]. Every arm is a *navigation*
/// gesture: it focuses or switches to the control, and never starts an edit
/// on the user's behalf — clicking "Annotate" must not commit a highlight the
/// moment the page loads.
///
/// A tool whose control this document refuses (an unsignable file, a
/// no-content-edit permission bit) is dropped here rather than reported: the
/// control's own disabled state and tooltip already say why, and
/// `ToggleButton::set_active` would otherwise take effect on an insensitive
/// widget.
pub(crate) fn apply(viewer: &Viewer, tool: HomeTool) {
    match tool {
        HomeTool::Edit => {
            if viewer.content_edit_button.is_sensitive() {
                viewer.content_edit_button.set_active(true);
            }
        }
        // Focusing rather than arming, and focusing a button rather than the
        // row, is also what scrolls the tools panel to reveal the section,
        // through GTK's usual focus-follows-scroll.
        HomeTool::Annotate => {
            if let Some((_, button)) = viewer.annotation_buttons.create.first() {
                button.grab_focus();
            }
        }
        HomeTool::Sign => {
            viewer.tools_stack.set_visible_child_name(FILL_SIGN_PAGE);
            viewer.choose_signing_certificate.grab_focus();
        }
        HomeTool::Organize => crate::app::organize::show(viewer),
    }
}

/// The three commands that need no open document, as flat rows.
pub(crate) fn build_quick_actions(window: &ApplicationWindow, viewer: &Viewer) -> GtkBox {
    let root = card("Quick actions");
    for action in QuickAction::ALL {
        let row = GtkBox::new(Orientation::Horizontal, 8);
        row.append(&build_icon(action.icon(), ROW_ICON_PX, ACCENT_TINT));
        let caption = Label::new(Some(action.label()));
        caption.set_xalign(0.0);
        row.append(&caption);

        let button = Button::new();
        button.set_child(Some(&row));
        button.add_css_class("home-link");
        button.set_halign(Align::Start);
        button.update_property(&[gtk::accessible::Property::Label(action.label())]);
        button.connect_clicked({
            let window = window.clone();
            let viewer = viewer.clone();
            move |_| action.run(&window, &viewer)
        });
        root.append(&button);
    }
    root
}

/// The commands worth reaching from Home without a document open. An enum
/// rather than an array of boxed closures: three named cases the compiler
/// checks are all handled, and no allocation to describe a button.
#[derive(Clone, Copy)]
enum QuickAction {
    NewBlank,
    Open,
    Sample,
}

impl QuickAction {
    const ALL: [QuickAction; 3] = [
        QuickAction::NewBlank,
        QuickAction::Open,
        QuickAction::Sample,
    ];

    fn label(self) -> &'static str {
        match self {
            QuickAction::NewBlank => "New blank PDF",
            QuickAction::Open => "Open file…",
            QuickAction::Sample => "Open the sample",
        }
    }

    fn icon(self) -> Icon {
        match self {
            QuickAction::NewBlank => Icon::NewFile,
            QuickAction::Open => Icon::Files,
            QuickAction::Sample => Icon::Sample,
        }
    }

    fn run(self, window: &ApplicationWindow, viewer: &Viewer) {
        match self {
            QuickAction::NewBlank => new_blank_document(window, viewer),
            QuickAction::Open => show_file_chooser(window, viewer),
            QuickAction::Sample => open_sample(window, viewer, SampleKind::Plain),
        }
    }
}

/// The accelerators `connect_standard_shortcuts` installs, written down.
///
/// A shell with no menu bar has nowhere else to show them, and the launch
/// screen is the one place the user is reading rather than working.
pub(crate) fn build_shortcuts_card() -> GtkBox {
    let root = card("Keyboard shortcuts");
    for (action, keys) in [
        ("Open", "Ctrl+O"),
        ("New", "Ctrl+N"),
        ("Save", "Ctrl+S"),
        ("Find", "Ctrl+F"),
        ("Print", "Ctrl+P"),
    ] {
        property_row(&root, action).set_text(keys);
    }
    root
}

/// A titled card. The three in this column share it so their padding, radius
/// and heading weight cannot drift apart.
fn card(title: &str) -> GtkBox {
    let root = GtkBox::new(Orientation::Vertical, 10);
    root.add_css_class("home-card");

    let heading = Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.add_css_class("home-card-title");
    root.append(&heading);

    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ui_tests::built_ui;

    fn tile(card: &ToolsCard, label: &str) -> Button {
        card.tiles
            .iter()
            .find(|(key, _)| key == label)
            .map(|(_, button)| button.clone())
            .unwrap_or_else(|| panic!("the grid must offer a {label} tile"))
    }

    /// Sections with no feature behind them stay visible and disabled, the
    /// same contract `shell::rail_item` holds for the rail.
    #[gtk::test]
    fn gtk_ui_tools_without_a_feature_are_disabled_not_missing() {
        let built = built_ui();
        let card = build_tools_card(&built.window, &built.viewer);

        assert!(tile(&card, "edit").is_sensitive());
        assert!(tile(&card, "sign").is_sensitive());
        assert!(tile(&card, "organize").is_sensitive());
        let compress = tile(&card, "compress");
        assert!(!compress.is_sensitive());
        assert_eq!(
            compress.tooltip_text().as_deref(),
            Some("Not available yet")
        );

        built.window.close();
    }

    /// A tile clicked with no document open arms the tool for the open that
    /// follows instead of doing nothing — the whole reason `pending_tool`
    /// exists.
    #[gtk::test]
    fn gtk_ui_a_tile_clicked_without_a_document_arms_the_tool_for_the_next_open() {
        let built = built_ui();
        let card = build_tools_card(&built.window, &built.viewer);

        assert!(built.viewer.state.borrow().pending_tool.is_none());
        tile(&card, "sign").emit_clicked();

        assert_eq!(
            built.viewer.state.borrow().pending_tool,
            Some(HomeTool::Sign)
        );

        built.window.close();
    }

    /// Filtering the grid to nothing takes the card with it, rather than
    /// leaving a heading over an empty box.
    #[gtk::test]
    fn gtk_ui_filtering_the_grid_to_nothing_hides_the_whole_card() {
        let built = built_ui();
        let card = build_tools_card(&built.window, &built.viewer);

        card.filter("sig");
        assert!(card.root.is_visible());
        assert!(tile(&card, "sign").is_visible());
        assert!(!tile(&card, "edit").is_visible());

        card.filter("nothing-matches-this");
        assert!(!card.root.is_visible());

        card.filter("");
        assert!(card.root.is_visible());
        assert!(tile(&card, "edit").is_visible());

        built.window.close();
    }
}
