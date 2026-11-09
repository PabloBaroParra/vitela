//! Pure hand-tool rules: which button grabs the document, and where a drag
//! leaves one scroll axis. No GTK types, so every edge is unit-tested.

/// The primary (left) mouse button, as GDK numbers it.
pub(super) const PRIMARY: u32 = 1;
/// The middle mouse button, as GDK numbers it.
pub(super) const MIDDLE: u32 = 2;

/// Whether a press of `button` starts a pan: the middle button always does,
/// the primary one only while Space is held. Everything else — and the
/// primary button without Space — belongs to the page tools.
pub(super) fn grabs(button: u32, space_held: bool) -> bool {
    button == MIDDLE || (button == PRIMARY && space_held)
}

/// One scroll axis' bounds, as a `gtk::Adjustment` reports them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Bounds {
    pub(super) lower: f64,
    pub(super) upper: f64,
    pub(super) page_size: f64,
}

/// The axis value after the pointer moved `offset` pixels since the drag
/// began at `start`.
///
/// The content follows the pointer, so dragging right or down *lowers* the
/// value. Clamped to `[lower, upper - page_size]`, collapsing to `lower` when
/// the page is larger than the content. A non-finite offset leaves the axis
/// where the drag started.
pub(super) fn panned(start: f64, offset: f64, bounds: Bounds) -> f64 {
    let max = (bounds.upper - bounds.page_size).max(bounds.lower);
    let target = if offset.is_finite() {
        start - offset
    } else {
        start
    };
    target.clamp(bounds.lower, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AXIS: Bounds = Bounds {
        lower: 0.0,
        upper: 1000.0,
        page_size: 200.0,
    };

    #[test]
    fn middle_button_grabs_with_or_without_space() {
        assert!(grabs(MIDDLE, false));
        assert!(grabs(MIDDLE, true));
    }

    #[test]
    fn primary_button_grabs_only_while_space_is_held() {
        assert!(!grabs(PRIMARY, false));
        assert!(grabs(PRIMARY, true));
    }

    #[test]
    fn secondary_and_extra_buttons_never_grab() {
        for button in [0, 3, 8, 9] {
            assert!(!grabs(button, true), "button {button}");
            assert!(!grabs(button, false), "button {button}");
        }
    }

    #[test]
    fn content_follows_the_pointer() {
        assert_eq!(panned(400.0, 30.0, AXIS), 370.0);
        assert_eq!(panned(400.0, -30.0, AXIS), 430.0);
        assert_eq!(panned(400.0, 0.0, AXIS), 400.0);
    }

    #[test]
    fn clamps_to_both_ends_of_the_axis() {
        assert_eq!(panned(100.0, 500.0, AXIS), 0.0);
        assert_eq!(panned(700.0, -500.0, AXIS), 800.0);
    }

    #[test]
    fn honours_a_non_zero_lower_bound() {
        let axis = Bounds {
            lower: 50.0,
            ..AXIS
        };
        assert_eq!(panned(60.0, 100.0, axis), 50.0);
    }

    #[test]
    fn a_page_larger_than_its_content_cannot_move() {
        let axis = Bounds {
            lower: 0.0,
            upper: 300.0,
            page_size: 500.0,
        };
        assert_eq!(panned(0.0, -80.0, axis), 0.0);
        assert_eq!(panned(0.0, 80.0, axis), 0.0);
    }

    #[test]
    fn a_non_finite_offset_keeps_the_start() {
        for offset in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(panned(400.0, offset, AXIS), 400.0);
        }
    }
}
