//! The staggered entrance and the global animation setting (checklist §11).
//!
//! A `#[gtk::test]` has no frame clock to sample, so nothing here asserts
//! what a card looks like halfway through its animation. What it can assert
//! is everything that decides whether the animation happens at all and how
//! long it is allowed to run: which cards carry the entrance classes, how
//! many of them do, and that none do when GTK's animations are off.

use super::*;
use crate::app::organize::motion::{animations_enabled, stagger_class, ENTER_CLASS};

/// More pages than the stagger has slots, so the cap is visible in the
/// result rather than merely asserted about the constant.
const PAGES: u32 = 20;

/// Restores `gtk-enable-animations` on the way out, including on an unwind.
/// `#[gtk::test]` bodies share one main thread (`gtk::test_synced`), so a
/// setting left switched off here is switched off for every test that runs
/// after it.
struct Animations(bool);

impl Animations {
    fn set(enabled: bool) -> Self {
        let settings = gtk::Settings::default().expect("gtk settings");
        let previous = settings.is_gtk_enable_animations();
        settings.set_gtk_enable_animations(enabled);
        Self(previous)
    }
}

impl Drop for Animations {
    fn drop(&mut self) {
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_enable_animations(self.0);
        }
    }
}

fn entering(viewer: &Viewer) -> Vec<usize> {
    viewer
        .organize
        .cards
        .snapshot()
        .iter()
        .enumerate()
        .filter(|(_, card)| card.root.has_css_class(ENTER_CLASS))
        .map(|(position, _)| position)
        .collect()
}

/// Arriving in a view fans its cards in, each one a little after the one
/// before it — the checklist's "escalonar la aparición de páginas".
#[gtk::test]
fn gtk_ui_arriving_in_the_pages_view_staggers_the_cards_it_shows() {
    let _animations = Animations::set(true);
    with_organize_of(PAGES, |viewer| {
        let cards = viewer.organize.cards.snapshot();

        for (position, card) in cards.iter().enumerate().take(3) {
            assert!(card.root.has_css_class(ENTER_CLASS), "card {position}");
            assert!(
                card.root.has_css_class(&stagger_class(position)),
                "card {position} must arrive in its own slot"
            );
        }
    });
}

/// The sequence's length cannot grow with the document. Past the last slot a
/// card simply is where it is — which on any reasonable window is below the
/// fold, and is the checklist's "animar solo elementos visibles".
#[gtk::test]
fn gtk_ui_the_stagger_stops_at_a_fixed_number_of_cards() {
    let _animations = Animations::set(true);
    with_organize_of(PAGES, |viewer| {
        let animated = entering(viewer);

        assert!(
            animated.len() < PAGES as usize,
            "a twenty-page document must not queue twenty delays"
        );
        assert_eq!(
            animated,
            (0..animated.len()).collect::<Vec<_>>(),
            "the animated cards are the first ones, in order"
        );
    });
}

/// "Cambiar inmediatamente de vista cuando las animaciones estén
/// desactivadas": with the setting off, the cards are simply there.
#[gtk::test]
fn gtk_ui_disabled_animations_leave_every_card_unanimated() {
    let _animations = Animations::set(false);
    with_organize_of(PAGES, |viewer| {
        assert!(!animations_enabled());
        assert_eq!(entering(viewer), Vec::<usize>::new());

        // And the setting is read per switch, not once at build time: turning
        // animations back on mid-session must take effect on the next view
        // the user asks for.
        let _on = Animations::set(true);
        viewer.organize.documents_toggle.set_active(true);
        viewer.organize.pages_toggle.set_active(true);

        assert_ne!(entering(viewer), Vec::<usize>::new());
    });
}

/// The block list's drop gaps are position markers, not content. A gap
/// fading in is a hole opening in the list.
#[gtk::test]
fn gtk_ui_the_documents_view_animates_its_blocks_and_not_its_drop_gaps() {
    let _animations = Animations::set(true);
    with_organize_of(PAGES, |viewer| {
        viewer.organize.documents_toggle.set_active(true);

        let mut blocks = 0;
        let mut child = viewer.organize.documents_list.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if widget.has_css_class("organize-block") {
                blocks += 1;
                assert!(widget.has_css_class(ENTER_CLASS), "a block must arrive");
            } else {
                assert!(
                    !widget.has_css_class(ENTER_CLASS),
                    "a drop gap must not animate"
                );
            }
        }
        assert_eq!(blocks, 1, "one source means one block");
    });
}

/// The view swap's own half of the animation, read off the `Stack` rather
/// than watched: a test that waited for a crossfade to finish would be a
/// test of this machine's frame rate.
#[gtk::test]
fn gtk_ui_the_view_stack_crossfades_for_the_configured_time() {
    with_organize(|viewer| {
        assert_eq!(
            viewer.organize.views.transition_type(),
            gtk::StackTransitionType::Crossfade
        );
        assert_eq!(
            viewer.organize.views.transition_duration(),
            crate::app::organize::motion::VIEW_TRANSITION_MS
        );
    });
}
