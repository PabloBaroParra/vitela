//! The Organize screen's motion: the transition between its two views and
//! the staggered entrance the cards make on arrival (checklist §11,
//! `docs/batch-pdf-assembly.md`).
//!
//! ## Why the entrance is applied from the view switch, not from `populate`
//!
//! Both views are rebuilt by more than a view switch — an undo, a redo, a
//! block move and a preview refresh all end in the same `populate` call. None
//! of those is an arrival, and animating them would make every Ctrl+Z flash
//! the whole grid. So the populate functions know nothing about motion, and
//! [`views::show`](super::views::show) — the one path that *is* "the user
//! asked for this view" — adds the entrance to whatever it has just built.
//! Nothing removes the classes again, and nothing has to: every arrival
//! rebuilds the cards it animates, so the class always lands on a widget that
//! has never carried it.
//!
//! ## Why the stagger is a fixed number of CSS classes
//!
//! GTK4 CSS has no inline styles, so a per-card delay has to be a class the
//! stylesheet already names. That bound is a feature here rather than a
//! workaround: the checklist asks for the sequence's total duration to stay
//! capped on large documents and for only visible elements to animate when
//! the full set would be costly. [`STAGGER_SLOTS`] answers both at once —
//! the first slots' worth of cards fan in, everything past them (which on
//! any reasonable window is below the fold) is simply there, and the longest
//! the whole sequence can run is the last delay plus one card's duration, no
//! matter how many pages the document has.

use gtk::prelude::*;
use gtk::{Stack, StackTransitionType, Widget};

use crate::app::state::Viewer;

/// How long the crossfade between Documents and Pages takes. Short: it is a
/// change of detail level on the same content, not a change of place.
pub(in crate::app::organize) const VIEW_TRANSITION_MS: u32 = 160;

/// How many cards fan in, and so how long the sequence can last: slot `n` is
/// delayed `n * 20ms` by the stylesheet, and the last one finishes 220ms +
/// one card's 180ms after the first starts. Five cards per row
/// (`super::CARDS_PER_ROW`) makes this a bit over two rows — about what is
/// on screen when the grid opens.
const STAGGER_SLOTS: usize = 12;

/// The class that carries the entrance keyframes. Public to the screen's
/// tests, which check *that* a card is animating rather than what it looks
/// like mid-flight — a `#[gtk::test]` has no frame clock to sample.
pub(in crate::app::organize) const ENTER_CLASS: &str = "organize-enter";

/// Sets up the view stack's own half of the transition. Called once, from
/// `super::build_organize_panel`.
///
/// The crossfade supplies the opacity the checklist asks for across the whole
/// view; the scale and the displacement are per-card, and live in the
/// `organize-enter` keyframes. GTK skips the transition by itself when
/// `gtk-enable-animations` is off, which is the stack's share of switching
/// immediately.
pub(in crate::app::organize) fn configure(views: &Stack) {
    views.set_transition_type(StackTransitionType::Crossfade);
    views.set_transition_duration(VIEW_TRANSITION_MS);
}

/// Whether GTK's global animation setting is on. The screen asks before every
/// entrance rather than caching the answer: it is a live setting, and a user
/// who turns animations off expects the next view switch to be instant, not
/// the next launch.
pub(in crate::app::organize) fn animations_enabled() -> bool {
    gtk::Settings::default().is_none_or(|settings| settings.is_gtk_enable_animations())
}

/// Gives the cards of the view now on show their staggered entrance, and
/// does nothing at all when animations are disabled — which leaves the cards
/// exactly as `populate` built them: on screen, at full opacity, immediately.
pub(in crate::app::organize) fn run_entrance(viewer: &Viewer) {
    if !animations_enabled() {
        return;
    }
    for (slot, widget) in entering(viewer).into_iter().take(STAGGER_SLOTS).enumerate() {
        widget.add_css_class(ENTER_CLASS);
        widget.add_css_class(&stagger_class(slot));
    }
}

/// The delay class for the `slot`-th card to arrive. Its rule lives in
/// `documents::style::ORGANIZE_CSS`, which names exactly [`STAGGER_SLOTS`] of
/// them.
pub(in crate::app::organize) fn stagger_class(slot: usize) -> String {
    format!("{ENTER_CLASS}-{slot}")
}

/// The cards of the view on show, in the order they should arrive.
///
/// The Documents list holds drop gaps between its cards (see
/// `super::documents`' header) and those must not animate: they are
/// position markers, not content, and a gap fading in is a hole opening in
/// the list.
fn entering(viewer: &Viewer) -> Vec<Widget> {
    if super::views::showing_documents(viewer) {
        let mut cards = Vec::new();
        let mut child = viewer.organize.documents_list.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if widget.has_css_class("organize-block") {
                cards.push(widget);
            }
        }
        return cards;
    }
    viewer
        .organize
        .cards
        .snapshot()
        .into_iter()
        .map(|card| card.root.upcast())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stagger_slot_the_screen_can_use_has_a_rule_in_the_stylesheet() {
        let css = super::super::ORGANIZE_CSS;
        for slot in 0..STAGGER_SLOTS {
            assert!(
                css.contains(&format!(".{} {{", stagger_class(slot))),
                "missing a delay rule for {}",
                stagger_class(slot)
            );
        }
        // One past the last is a class no card can be given; a rule for it
        // would mean the stylesheet and the stagger disagree about how long
        // the sequence is allowed to run.
        assert!(!css.contains(&format!(".{} {{", stagger_class(STAGGER_SLOTS))));
    }
}
