//! The palette's widgets (B27 T-230): a layer over the whole window, a card
//! top-centred on it, a search field and the list of commands.
//!
//! Widgets only. What opening, typing, the keys and a click do lives in
//! `palette/mod.rs`.
//!
//! A `ListBox`, not a `ListView`: the catalog is ~35 rows, rebuilt whole on
//! every keystroke for a few microseconds' work, and each row is a small
//! box of labels. A `ListView` would add a model, a factory and per-row
//! binding for recycling that 35 rows never need.

use gtk::prelude::*;
use gtk::{
    gdk, pango, Align, Box as GtkBox, Label, ListBox, ListBoxRow, Orientation, PolicyType,
    ScrolledWindow, SearchEntry, SelectionMode,
};

use super::entries::{Availability, PaletteEntry};
use crate::app::i18n::tr;
use crate::app::shortcuts::accel_label;

/// Set on the row of a refused entry, which stays selectable so its reason
/// can be read.
pub(super) const REFUSED_CLASS: &str = "palette-refused";

/// The card's look, from the shared palette (`theme`) so light and dark
/// both follow the system; the shape follows the content-edit quick-action
/// bar, the other card that floats over the window.
pub(crate) const PALETTE_CSS: &str = r#"
.palette-layer {
  background: alpha(@vitela_window, 0.55);
}

.palette-card {
  background: @vitela_surface;
  color: @vitela_text;
  border: 1px solid @vitela_border;
  border-radius: 10px;
  padding: 8px;
  box-shadow: 0 8px 24px alpha(@vitela_border_strong, 0.6);
}

.palette-card entry {
  min-height: 34px;
  border: 1px solid @vitela_input_border;
  border-radius: 6px;
  background: @vitela_input;
  color: @vitela_text;
  caret-color: @vitela_text;
}

.palette-card list {
  background: transparent;
}

.palette-card list > row {
  padding: 6px 10px;
  border-radius: 6px;
}

.palette-card list > row:hover {
  background: @vitela_hover;
}

.palette-card list > row:selected {
  background: @vitela_accent_soft;
  color: @vitela_text;
}

.palette-shortcut {
  color: @vitela_text_muted;
  font-size: 0.9em;
}

.palette-refused .palette-label {
  color: @vitela_text_disabled;
}

.palette-reason,
.palette-empty {
  color: @vitela_text_muted;
  font-size: 0.85em;
}

.palette-empty {
  padding: 12px;
}
"#;

/// The palette's widgets. `layer` is what goes on the window's root
/// overlay; it is hidden while the palette is closed.
pub(super) struct View {
    /// Covers the whole window, so a click outside the card lands here.
    pub(super) layer: GtkBox,
    pub(super) card: GtkBox,
    pub(super) search: SearchEntry,
    pub(super) list: ListBox,
    pub(super) scroller: ScrolledWindow,
    /// Shown instead of the list when nothing matches.
    pub(super) empty: Label,
}

pub(super) fn build() -> View {
    let search = SearchEntry::new();
    search.set_placeholder_text(Some(&tr("palette-search-placeholder")));
    // A natural width, not a minimum: a narrow window shrinks the card.
    search.set_max_width_chars(48);
    search.set_hexpand(true);

    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::Single);
    list.set_activate_on_single_click(true);
    let scroller = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(380)
        .child(&list)
        .build();

    let empty = Label::new(Some(&tr("palette-no-results")));
    empty.add_css_class("palette-empty");
    empty.set_visible(false);

    let card = GtkBox::new(Orientation::Vertical, 6);
    card.add_css_class("palette-card");
    card.set_halign(Align::Center);
    card.set_valign(Align::Start);
    card.set_margin_top(56);
    card.set_margin_start(16);
    card.set_margin_end(16);
    card.update_property(&[gtk::accessible::Property::Label(&tr("palette-title"))]);
    card.append(&search);
    card.append(&scroller);
    card.append(&empty);

    let layer = GtkBox::new(Orientation::Vertical, 0);
    layer.add_css_class("palette-layer");
    layer.set_halign(Align::Fill);
    layer.set_valign(Align::Fill);
    layer.set_visible(false);
    layer.append(&card);

    View {
        layer,
        card,
        search,
        list,
        scroller,
        empty,
    }
}

/// One command's row: its label in the active language, its shortcut when
/// it has one, and — refused — the reason, under a dimmed label.
pub(super) fn row(entry: &PaletteEntry, availability: &Availability) -> ListBoxRow {
    let label = Label::new(Some(&tr(entry.label_key)));
    label.add_css_class("palette-label");
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_ellipsize(pango::EllipsizeMode::End);
    let line = GtkBox::new(Orientation::Horizontal, 12);
    line.append(&label);
    if let Some(text) = entry.shortcut.and_then(accel_label) {
        let shortcut = Label::new(Some(&text));
        shortcut.add_css_class("palette-shortcut");
        line.append(&shortcut);
    }
    let column = GtkBox::new(Orientation::Vertical, 2);
    column.append(&line);

    let row = ListBoxRow::new();
    // The cursor stays in the search field: arrows and Enter are read
    // there, and a click on a row must not pull the focus into the list.
    row.set_focusable(false);
    row.set_child(Some(&column));
    if let Availability::Refused(reason) = availability {
        let reason = Label::new(Some(reason));
        reason.add_css_class("palette-reason");
        reason.set_xalign(0.0);
        reason.set_wrap(true);
        reason.set_max_width_chars(52);
        column.append(&reason);
        row.add_css_class(REFUSED_CLASS);
        row.update_state(&[gtk::accessible::State::Disabled(true)]);
    }
    row
}

/// What a key pressed in the palette does, if anything special.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Key {
    Up,
    Down,
    Close,
}

pub(super) fn key_action(key: gdk::Key) -> Option<Key> {
    match key {
        gdk::Key::Up | gdk::Key::KP_Up => Some(Key::Up),
        gdk::Key::Down | gdk::Key::KP_Down => Some(Key::Down),
        gdk::Key::Escape => Some(Key::Close),
        _ => None,
    }
}

/// Scrolls the list just enough to show `row`.
pub(super) fn reveal(view: &View, row: &ListBoxRow) {
    let Some(bounds) = row.compute_bounds(&view.list) else {
        return;
    };
    let adjustment = view.scroller.vadjustment();
    let top = f64::from(bounds.y());
    let bottom = top + f64::from(bounds.height());
    if top < adjustment.value() {
        adjustment.set_value(top);
    } else if bottom > adjustment.value() + adjustment.page_size() {
        adjustment.set_value(bottom - adjustment.page_size());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_move_and_escape_closes() {
        assert_eq!(key_action(gdk::Key::Up), Some(Key::Up));
        assert_eq!(key_action(gdk::Key::KP_Down), Some(Key::Down));
        assert_eq!(key_action(gdk::Key::Escape), Some(Key::Close));
        assert_eq!(key_action(gdk::Key::a), None);
        assert_eq!(key_action(gdk::Key::Return), None);
    }

    /// The text of the row's shortcut label, if it shows one.
    fn shortcut_shown(row: &ListBoxRow) -> Option<String> {
        let column = row.child()?.downcast::<GtkBox>().ok()?;
        let line = column.first_child()?.downcast::<GtkBox>().ok()?;
        let mut child = line.first_child();
        while let Some(widget) = child {
            if widget.has_css_class("palette-shortcut") {
                return widget
                    .downcast::<Label>()
                    .ok()
                    .map(|label| label.text().into());
            }
            child = widget.next_sibling();
        }
        None
    }

    /// The palette and Home's shortcut card print the same key the same
    /// way: both go through `shortcuts::accel_label` (B27 decision 7).
    #[gtk::test]
    fn gtk_ui_palette_and_home_card_spell_a_shortcut_alike() {
        use crate::app::home::tools::{
            build_shortcuts_card, test_support::card_rows, SHORTCUT_CARD,
        };
        use crate::app::shortcuts::COMMAND_PALETTE;

        let card = card_rows(&build_shortcuts_card());
        assert_eq!(card.len(), SHORTCUT_CARD.len());
        let catalog = crate::app::palette::entries::catalog();
        let mut compared = 0;
        for ((_, shortcut), (label, on_card)) in SHORTCUT_CARD.iter().zip(&card) {
            let Some(entry) = catalog
                .iter()
                .find(|entry| entry.shortcut == Some(shortcut.accel))
            else {
                // Ctrl+K: the palette does not list itself.
                assert_eq!(*shortcut, COMMAND_PALETTE);
                assert_eq!(Some(on_card.clone()), accel_label(shortcut.accel));
                continue;
            };
            let on_palette = shortcut_shown(&row(entry, &Availability::Available));
            assert_eq!(on_palette.as_deref(), Some(on_card.as_str()), "{label}");
            compared += 1;
        }
        assert_eq!(compared, SHORTCUT_CARD.len() - 1, "all but Ctrl+K");
    }
}
