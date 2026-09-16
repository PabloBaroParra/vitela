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
use crate::app::tools_panel::{property_row, ANNOTATE_PAGE, EDIT_PAGE, FILL_SIGN_PAGE};

/// One tile: its label, the tool it opens (`None` for a section this shell
/// has no feature behind yet), its icon and accent, and what it is for.
///
/// A `None` entry is kept visible and disabled rather than dropped — a
/// disabled control with a tooltip says "later", an absent one says "never",
/// and only one of those is true. Its accent is carried here all the same:
/// [`tile_tint`] decides whether a tile is coloured, so the palette stays one
/// table rather than a colour in one place and an exception in another.
///
/// Since T-199 every row in [`TOOLS`] is live, so the treatment is reached
/// only by a row a future section adds. It is still exercised — see
/// [`build_tile`] for why that needed a function of its own.
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
        tool: Some(HomeTool::Compress),
        icon: Icon::Compress,
        tint: COMPRESS_TINT,
        description: "Write a smaller copy of the file",
    },
    ToolTile {
        label: "Protect",
        tool: Some(HomeTool::Protect),
        icon: Icon::Protect,
        tint: PROTECT_TINT,
        description: "Require a password to open the document",
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
            let tile = build_tile(window, viewer, entry);
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

/// One tile, built from its table row.
///
/// Its own function rather than a closure inside [`build_tools_card`], and
/// T-199 is why: enabling Compress left `TOOLS` with no `None` row in it, so
/// the disabled branch below — and the "visible but disabled" contract it
/// holds — lost the only tile a test could reach it through. A tile builder
/// that takes a [`ToolTile`] can be handed one the table does not contain,
/// which is what `a_tool_with_no_feature_behind_it_is_disabled_not_missing`
/// does. The branch is not dead: it is the treatment every section this shell
/// has not built yet still gets, and the next one lands as a table row rather
/// than as a re-argued decision.
fn build_tile(window: &ApplicationWindow, viewer: &Viewer, entry: &ToolTile) -> Button {
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
    tile
}

/// A tile's icon colour: its own accent when the tool is live, the muted grey
/// when it is not.
///
/// A full-strength brand colour inside a greyed-out tile is the one signal on
/// the card that says "click me" — which is exactly what the disabled state
/// exists to deny. Kept as a rule rather than folded away now that every row
/// is live, for the same reason the disabled branch is.
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
        // The page switch is unconditional, unlike the arming below. A
        // document whose permission bits refuse content changes used to make
        // this arm do nothing whatsoever — the rail's "Edit PDF" button
        // accepted the click and the shell did not move — because the only
        // thing here was `set_active` on a control that was insensitive. The
        // Edit page states that refusal in words (`content_edit::panel`), so
        // going there is exactly what a refused document needs to do.
        HomeTool::Edit => {
            viewer.tools_stack.set_visible_child_name(EDIT_PAGE);
            if viewer.content_edit_button.is_sensitive() {
                viewer.content_edit_button.set_active(true);
            }
        }
        // Focusing rather than arming, and focusing a button rather than the
        // row, is also what scrolls the tools panel to reveal the section,
        // through GTK's usual focus-follows-scroll. The page switch has to
        // come first: focus on a widget sitting in a `Stack` page that is not
        // the visible one reveals nothing, and the annotation toolbar shares
        // the panel with three other pages now.
        HomeTool::Annotate => {
            viewer.tools_stack.set_visible_child_name(ANNOTATE_PAGE);
            if let Some((_, button)) = viewer.annotation_buttons.create.first() {
                button.grab_focus();
            }
        }
        HomeTool::Sign => {
            viewer.tools_stack.set_visible_child_name(FILL_SIGN_PAGE);
            viewer.choose_signing_certificate.grab_focus();
        }
        HomeTool::Organize => crate::app::organize::show(viewer),
        // The one arm that is not navigation. It stays a "tool" all the same:
        // from a cold start the gesture is pick a file, land on Protect, and
        // routing it anywhere else would mean a second dispatch table for a
        // single entry. `begin_protect` reports its own refusals, so unlike
        // the arming arms above there is nothing to drop silently here.
        HomeTool::Protect => crate::app::protect::begin_protect(viewer),
        // The second arm that is not navigation, and it sits next to `Protect`
        // for the reason that one does: "compress the document I just picked"
        // is a cold-start gesture, and `begin_compress` reports its own
        // refusals, so there is nothing to drop silently here either.
        HomeTool::Compress => crate::app::write::begin_compress(viewer),
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

    /// Every row in the table is live, and each one says what it is for.
    ///
    /// Compress was the last `None` in [`TOOLS`] until T-199, and this case
    /// used to be where its disabled state was pinned. The contract itself did
    /// not go away with the row — it moved to the case below, which reaches
    /// the same branch through a tile the table does not contain. What is
    /// asserted here instead is the thing that replaced it: a live tile has a
    /// tooltip that describes the tool rather than apologises for it, and an
    /// empty `description` would be a silent regression otherwise.
    #[gtk::test]
    fn gtk_ui_every_tool_tile_is_live_and_says_what_it_does() {
        let built = built_ui();
        let card = build_tools_card(&built.window, &built.viewer);

        for entry in TOOLS.iter() {
            let tile = tile(&card, &entry.label.to_lowercase());
            assert!(tile.is_sensitive(), "{} must be live", entry.label);
            assert_eq!(
                tile.tooltip_text().as_deref(),
                Some(entry.description),
                "{} must describe itself",
                entry.label
            );
            assert!(
                !entry.description.is_empty(),
                "{} must have something to describe",
                entry.label
            );
        }

        built.window.close();
    }

    /// The "visible but disabled" contract, kept alive after its last tile
    /// went live.
    ///
    /// A section this shell has not built yet is shown greyed out with a
    /// tooltip, never dropped from the grid — the treatment Recent, Organize,
    /// Protect and finally Compress each had in turn. `shell::rail_item` made
    /// the opposite call when its own last `false` disappeared and deleted the
    /// branch; here the branch stays, because the grid is the one place the
    /// shell still advertises what is coming. Driven through [`build_tile`] on
    /// a row of this test's own so the rule survives the table being fully
    /// populated.
    #[gtk::test]
    fn gtk_ui_a_tool_with_no_feature_behind_it_is_disabled_not_missing() {
        let built = built_ui();
        let unbuilt = ToolTile {
            label: "Later",
            tool: None,
            icon: Icon::Compress,
            tint: COMPRESS_TINT,
            description: "",
        };

        let tile = build_tile(&built.window, &built.viewer, &unbuilt);

        // `get_visible`, not `is_visible`: the latter answers for the whole
        // ancestor chain, and this tile was never put in one.
        assert!(tile.get_visible());
        assert!(!tile.is_sensitive());
        assert_eq!(tile.tooltip_text().as_deref(), Some("Not available yet"));

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

    /// **"Edit PDF" has to go somewhere.**
    ///
    /// Before the Edit page existed, this arm's whole effect was arming one
    /// toggle inside the tab next door — and when that toggle was insensitive
    /// (no document open, or one whose permission bits refuse content
    /// changes) the rail button accepted the click and nothing moved on
    /// screen at all. The page switch is what the user gets instead, and it
    /// is deliberately not conditional on the toggle: the page is where the
    /// refusal is written down.
    #[gtk::test]
    fn gtk_ui_the_edit_tool_opens_the_edit_page_even_when_editing_is_unavailable() {
        let built = built_ui();
        assert!(
            !built.viewer.content_edit_button.is_sensitive(),
            "this test is meaningless unless editing starts unavailable"
        );

        apply(&built.viewer, HomeTool::Edit);

        assert_eq!(
            built.viewer.tools_stack.visible_child_name().as_deref(),
            Some(EDIT_PAGE)
        );
        assert!(!built.viewer.content_edit_button.is_active());

        built.window.close();
    }

    /// Annotate reveals a control by focusing it, and focus reveals nothing
    /// on a `Stack` page that is not the visible one — so it has to bring the
    /// Annotate page back up first, from whichever page the panel was left on.
    #[gtk::test]
    fn gtk_ui_the_annotate_tool_returns_to_the_annotate_page() {
        let built = built_ui();
        apply(&built.viewer, HomeTool::Edit);

        apply(&built.viewer, HomeTool::Annotate);

        assert_eq!(
            built.viewer.tools_stack.visible_child_name().as_deref(),
            Some(ANNOTATE_PAGE)
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
