//! The "Edit" page of the right-hand tools panel: the surface the app rail's
//! "Edit PDF" button and Home's "Edit" tile now land on.
//!
//! ## Why a page of its own
//!
//! These five controls used to be one unlabelled `FlowBox` row appended under
//! a "Content" heading in the tab next door (called "Tools" then, "Annotate"
//! now), between the annotation toolbar above and the document-properties
//! panel below. Three problems came with that, and this module exists to fix
//! all three:
//!
//! 1. **"Edit PDF" went nowhere.** The rail button's whole effect was
//!    `content_edit_button.set_active(true)` — a single toggle turning itself
//!    on somewhere inside a scrolling column of a dozen other controls, with
//!    nothing moving on screen to say where. On a document that refuses
//!    content changes it did *nothing at all*, silently, because
//!    `home::tools::apply` skips an insensitive control. Now the rail always
//!    lands on this page, and the page always says what state it is in.
//! 2. **Nothing said what the mode does.** "Edit content" is a mode: it
//!    changes what a *page click* means. The only place that was ever
//!    explained was the status bar, one line, replaced by the next message.
//!    Here each group carries its gesture in writing, permanently.
//! 3. **"Delete image"/"Replace image" looked broken.** They are gated on an
//!    image being selected on the page, so they sit greyed out for as long as
//!    nothing is, with no stated reason. [`EditPanel::image_hint`] is that
//!    reason, and it tracks the selection.
//!
//! ## Shape
//!
//! Heading, one-line intro, a notice, then a card per half of page content —
//! text and images — each with its controls and the gesture that drives them.
//! The cards, the radius and the muted hint type are Home's and the Organize
//! screen's, so the three read as one application rather than three authors.
//!
//! The canvas stays where it is, which is why this is a panel page and not a
//! fourth `view_stack` screen like `organize`: every gesture described here
//! is a *click on the page*, so the page has to be on screen to make it.

use gtk::prelude::*;
use gtk::{Align, Box as GtkBox, Button, FlowBox, Label, Orientation, SelectionMode, ToggleButton};

use crate::app::icons::{build_icon, Icon, ACCENT_TINT, MUTED_TINT};
use crate::app::state::EditPanel;
use crate::app::tools_panel::panel_heading;

/// Icon edge on a tile. Sized against the label beside it, the same rule
/// `shell::RAIL_ICON_PX` follows for the rail — these read as a list of
/// commands, not as a launch grid of destinations like Home's 24px tiles.
const EDIT_ICON_PX: i32 = 16;

/// Shown while there is no document to edit. Stated rather than left to the
/// greyed-out controls, which say "not now" without ever saying why.
pub(crate) const NO_DOCUMENT_NOTICE: &str = "Open a PDF to edit the text and images on its pages.";

/// What the image controls need before they can do anything, and what they
/// can do once they have it. Maintained by
/// `crate::app::update_content_edit_controls`, off the same selection that
/// gates the buttons themselves, so the sentence and the sensitivity can
/// never disagree.
pub(crate) const NO_IMAGE_SELECTED: &str = "Click an image on the page to select it.";
pub(crate) const IMAGE_SELECTED: &str = "Image selected — replace or delete it.";

/// This page's own styling, installed alongside `shell::SHELL_CSS` and
/// `home::HOME_CSS` by `shell::install_shell_css`.
///
/// The palette is the shell's exactly — `#6b4eff` accent, `#e3e0e9`
/// hairlines, `#625b72` secondary text. The classes are this page's own
/// rather than Home's `.home-card`: a card in the tools panel is a narrower
/// box with tighter padding than one on a full-width launch screen, and
/// borrowing the name would make every future tweak to Home's card silently
/// resize this one.
pub(crate) const EDIT_CSS: &str = r#"
.edit-card {
  background: #ffffff;
  border: 1px solid #e3e0e9;
  border-radius: 12px;
  padding: 12px;
}

.edit-card-title {
  font-weight: 700;
  color: #302d3a;
}

.edit-hint {
  font-size: 0.85em;
  color: #625b72;
}

.edit-notice {
  background: #f6f4fd;
  border: 1px solid #e7e2fb;
  border-radius: 8px;
  padding: 8px 10px;
  color: #51496a;
  font-size: 0.85em;
}

.edit-tile {
  background: #f6f4fd;
  border: 1px solid #e7e2fb;
  border-radius: 10px;
  padding: 6px 10px;
  color: #51496a;
  font-weight: 600;
  transition: background-color 120ms ease, border-color 120ms ease;
}

.edit-tile:hover,
.edit-tile:focus-visible {
  background: #eee9fa;
}

/* An armed mode, not merely a pressed button: the border moves to the accent
   too, because "is this mode on right now" is the one question this page is
   asked most and a background shift alone is easy to miss beside a hover. */
.edit-tile:checked {
  background: #eee9fa;
  border-color: #6b4eff;
  color: #6b4eff;
}

.edit-tile:disabled {
  background: #f5f4f7;
  border-color: #eae8ef;
  color: #a49fb3;
}
"#;

/// The page's controls, handed to `build_ui` to place on the `Viewer`.
///
/// The five buttons keep their existing homes as flat `Viewer` fields —
/// `content_edit`, `image` and `home::tools` all address them by name — so
/// this struct only carries them across the module boundary. The two labels
/// are new, and travel together in [`EditPanel`].
pub(crate) struct EditContent {
    pub(crate) mode: ToggleButton,
    pub(crate) insert_text: ToggleButton,
    pub(crate) insert_image: ToggleButton,
    pub(crate) delete_image: Button,
    pub(crate) replace_image: Button,
    pub(crate) panel: EditPanel,
}

/// Builds the Edit page and the controls on it. Nothing is wired here: the
/// mode and insert toggles are connected by `content_edit::connect_toggle`/
/// `connect_insert_toggles`, and the two image buttons by `build_ui`, exactly
/// as they were before this page existed.
pub(crate) fn build_edit_content() -> (EditContent, GtkBox) {
    let root = GtkBox::new(Orientation::Vertical, 10);

    root.append(&panel_heading("Edit PDF"));
    root.append(&hint("Change the text and images already on the page."));

    let availability = Label::new(Some(NO_DOCUMENT_NOTICE));
    availability.set_wrap(true);
    availability.set_xalign(0.0);
    availability.add_css_class("edit-notice");
    root.append(&availability);

    // --- text --------------------------------------------------------------
    let (text_card, text_row) = card("Text");
    let mode = ToggleButton::new();
    tile(
        mode.upcast_ref(),
        "Edit content",
        Icon::Edit,
        "Turn on content editing, then click a text run on the page",
    );
    mode.set_sensitive(false);
    let insert_text = ToggleButton::new();
    tile(
        insert_text.upcast_ref(),
        "Insert text",
        Icon::Text,
        "Click the page to place a new text box",
    );
    insert_text.set_sensitive(false);
    text_row.append(&mode);
    text_row.append(&insert_text);
    text_card.append(&hint(
        "Click a text run to retype it in place, or drag it to move it.",
    ));
    root.append(&text_card);

    // --- images ------------------------------------------------------------
    let (image_card, image_row) = card("Images");
    let insert_image = ToggleButton::new();
    tile(
        insert_image.upcast_ref(),
        "Insert image",
        Icon::Image,
        "Click the page to insert a picture",
    );
    insert_image.set_sensitive(false);
    let replace_image = Button::new();
    tile(
        &replace_image,
        "Replace image",
        Icon::Image,
        "Swap the selected image for a file on disk",
    );
    replace_image.set_sensitive(false);
    let delete_image = Button::new();
    tile(
        &delete_image,
        "Delete image",
        Icon::Delete,
        "Remove the selected image from the page",
    );
    delete_image.set_sensitive(false);
    image_row.append(&insert_image);
    image_row.append(&replace_image);
    image_row.append(&delete_image);
    let image_hint = hint(NO_IMAGE_SELECTED);
    image_card.append(&image_hint);
    root.append(&image_card);

    (
        EditContent {
            mode,
            insert_text,
            insert_image,
            delete_image,
            replace_image,
            panel: EditPanel {
                availability,
                image_hint,
            },
        },
        root,
    )
}

/// One titled card and the `FlowBox` its controls go in.
///
/// A `FlowBox` and not a horizontal `GtkBox`, for the reason
/// `editor_toolbar` documents at length and this shell has now relearned
/// three times: a horizontal box reports the *sum* of its children as its own
/// minimum width, and a minimum inside the tools panel becomes the floor the
/// canvas/tools divider cannot be dragged past. Three tiles in a row must be
/// able to become three rows of one.
fn card(title: &str) -> (GtkBox, FlowBox) {
    let card = GtkBox::new(Orientation::Vertical, 8);
    card.add_css_class("edit-card");

    let heading = Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.add_css_class("edit-card-title");
    card.append(&heading);

    let row = FlowBox::new();
    row.set_selection_mode(SelectionMode::None);
    row.set_homogeneous(false);
    row.set_row_spacing(6);
    row.set_column_spacing(6);
    card.append(&row);

    (card, row)
}

fn hint(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.add_css_class("edit-hint");
    label
}

/// Dresses `button` — a `Button` or a `ToggleButton` — as an icon-and-label
/// tile with `tooltip` on it.
///
/// The icon is re-tinted whenever the button's sensitivity changes, rather
/// than painted once at build time. Every control on this page is gated on
/// something (a document being open, its permission bits, an image being
/// selected), so a fixed accent would leave a full-strength brand colour
/// inside a greyed-out tile — the one signal on the card still saying "click
/// me", which is exactly what the disabled state exists to deny. Home's grid
/// solves the same problem with `tools::tile_tint`, but its answer is static
/// because a tool tile's availability never changes after build; here it
/// changes on every document open and every image click.
fn tile(button: &Button, label: &str, icon: Icon, tooltip: &str) {
    let content = GtkBox::new(Orientation::Horizontal, 8);
    content.set_halign(Align::Start);
    content.append(&build_icon(icon, EDIT_ICON_PX, ACCENT_TINT));
    content.append(&Label::new(Some(label)));

    button.set_child(Some(&content));
    button.add_css_class("edit-tile");
    button.set_tooltip_text(Some(tooltip));
    // A button given a custom child has no label of its own for the
    // accessibility layer to fall back on, so it is stated rather than
    // inferred from whichever descendant happens to hold text.
    button.update_property(&[gtk::accessible::Property::Label(label)]);

    let repaint = {
        let content = content.clone();
        move |sensitive: bool| {
            if let Some(previous) = content.first_child() {
                content.remove(&previous);
            }
            let tint = if sensitive { ACCENT_TINT } else { MUTED_TINT };
            content.prepend(&build_icon(icon, EDIT_ICON_PX, tint));
        }
    };
    repaint(button.is_sensitive());
    button.connect_notify_local(Some("sensitive"), move |button, _| {
        repaint(button.is_sensitive());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every control on this page starts unusable, because every one of them
    /// needs a document — and the page says so in words rather than leaving
    /// five greyed tiles to be interpreted.
    #[gtk::test]
    fn gtk_ui_the_page_opens_disabled_and_explains_why() {
        let (content, _root) = build_edit_content();

        assert!(!content.mode.is_sensitive());
        assert!(!content.insert_text.is_sensitive());
        assert!(!content.insert_image.is_sensitive());
        assert!(!content.delete_image.is_sensitive());
        assert!(!content.replace_image.is_sensitive());
        assert_eq!(content.panel.availability.text(), NO_DOCUMENT_NOTICE);
        assert_eq!(content.panel.image_hint.text(), NO_IMAGE_SELECTED);
    }

    /// The regression [`tile`]'s doc exists for: a disabled tile must not keep
    /// a full-strength accent icon in it. Asserted through the widget the
    /// repaint replaces — a fresh `Image` each time — rather than by reading
    /// pixels, which `icons`' own optical-grid test already covers.
    #[gtk::test]
    fn gtk_ui_a_tile_repaints_its_icon_when_its_sensitivity_changes() {
        let (content, _root) = build_edit_content();
        let icon_of = |button: &ToggleButton| {
            button
                .child()
                .and_then(|child| child.downcast::<GtkBox>().ok())
                .and_then(|content| content.first_child())
                .expect("a tile leads with its icon")
        };

        let disabled = icon_of(&content.mode);
        content.mode.set_sensitive(true);
        let enabled = icon_of(&content.mode);

        assert!(
            disabled != enabled,
            "the tile kept the icon it was painted while disabled"
        );
    }

    /// Each card's controls wrap instead of setting the panel's minimum
    /// width. Same property `editor_toolbar` and `tools_panel`'s tab strip
    /// each pin for their own row — a horizontal `GtkBox` here would make the
    /// widest card the floor the canvas/tools divider cannot pass.
    #[gtk::test]
    fn gtk_ui_a_card_row_wraps_instead_of_setting_the_panels_floor() {
        let (_card, row) = card("Images");
        for label in ["Insert image", "Replace image", "Delete image"] {
            let button = Button::new();
            tile(&button, label, Icon::Image, label);
            row.append(&button);
        }

        let (minimum, _, _, _) = row.measure(Orientation::Horizontal, -1);
        let sum_of_tiles: i32 =
            std::iter::successors(row.first_child(), |child| child.next_sibling())
                .map(|tile| tile.measure(Orientation::Horizontal, -1).1)
                .sum();

        assert!(
            minimum * 2 <= sum_of_tiles,
            "card row minimum {minimum} is not meaningfully below the {sum_of_tiles} its tiles \
             add up to; it is behaving like a plain GtkBox and will floor the tools panel"
        );
    }
}
