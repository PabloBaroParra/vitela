//! Pixels measured against paper.
//!
//! ## The rule this module exists to keep
//!
//! *When one image is drawn more than once, the **largest** placement
//! governs.*
//!
//! An image's resolution is not a property of the image. A 1 000-pixel-wide
//! photograph placed one inch across is 1 000 DPI; the same object placed ten
//! inches across is 100. So "is this image too detailed" is a question about
//! a *placement*, and a document can answer it several times for the same
//! object.
//!
//! When it does, the answers must not be averaged and must not be taken in
//! document order. Resampling to satisfy the one-inch placement would leave
//! the ten-inch placement drawing 150 pixels across ten inches — fifteen DPI,
//! visibly destroyed — while resampling to satisfy the ten-inch placement
//! merely leaves the small one sharper than it needed to be. The largest
//! placement is the one that can be ruined, so it is the one that decides.
//!
//! In effective-DPI terms that is the **lowest** number, which is why
//! [`EffectiveDpi::governed_by`] is a minimum. Per axis, and separately: a
//! matrix may scale width and height differently, and the image's own *u*
//! axis stays its *u* axis however the placement turns it.

/// How many source samples land on an inch of paper, along each of the
/// image's own axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EffectiveDpi {
    /// Across the image's width — the axis a `cm` maps to `(a, b)`.
    pub(crate) horizontal: f64,
    /// Across the image's height — the axis a `cm` maps to `(c, d)`.
    pub(crate) vertical: f64,
}

/// A PDF user-space unit is 1/72 inch (PDF 32000-1 8.3.2.3), which is the
/// only reason a DPI can be read off a matrix at all.
const UNITS_PER_INCH: f64 = 72.0;

impl EffectiveDpi {
    /// `pixels` samples spread across `drawn` user-space units.
    ///
    /// `None` when the placement draws nothing measurable: a degenerate `cm`
    /// that collapses an axis to zero, a non-finite one, or an image with no
    /// samples along an axis. Such a placement paints nothing, so it must not
    /// be allowed to govern — and dividing by it would produce an infinity
    /// that a minimum would happily keep.
    pub(crate) fn measure(pixels: (u32, u32), drawn: (f64, f64)) -> Option<EffectiveDpi> {
        let along = |samples: u32, units: f64| {
            (samples > 0 && units.is_finite() && units > 0.0)
                .then(|| f64::from(samples) * UNITS_PER_INCH / units)
        };

        Some(EffectiveDpi {
            horizontal: along(pixels.0, drawn.0)?,
            vertical: along(pixels.1, drawn.1)?,
        })
    }

    /// This measurement and `other`, resolved to the one that must be
    /// satisfied — see this module's header.
    pub(crate) fn governed_by(self, other: EffectiveDpi) -> EffectiveDpi {
        EffectiveDpi {
            horizontal: self.horizontal.min(other.horizontal),
            vertical: self.vertical.min(other.vertical),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dpi(pixels: (u32, u32), drawn: (f64, f64)) -> EffectiveDpi {
        EffectiveDpi::measure(pixels, drawn).expect("a real placement measures")
    }

    /// The definition, at the one scale where it is legible by eye: 72 user
    /// units is an inch, so 72 samples across it is 72 DPI.
    #[test]
    fn an_image_drawn_at_one_sample_per_unit_is_seventy_two_dpi() {
        assert_eq!(
            dpi((72, 72), (72.0, 72.0)),
            EffectiveDpi {
                horizontal: 72.0,
                vertical: 72.0
            }
        );
    }

    #[test]
    fn halving_the_drawn_size_doubles_the_effective_dpi() {
        assert_eq!(dpi((300, 300), (150.0, 150.0)).horizontal, 144.0);
        assert_eq!(dpi((300, 300), (75.0, 75.0)).horizontal, 288.0);
    }

    /// A placement is free to stretch one axis and not the other, so the two
    /// numbers are measured independently rather than from an average.
    #[test]
    fn a_stretched_placement_measures_its_two_axes_apart() {
        let measured = dpi((300, 300), (150.0, 300.0));

        assert_eq!(measured.horizontal, 144.0);
        assert_eq!(measured.vertical, 72.0);
    }

    /// The rule this module exists for. 100 DPI is the ten-inch placement and
    /// 1 000 the one-inch one; resampling to 1 000 would leave the first
    /// drawing at a fraction of its size.
    #[test]
    fn the_larger_placement_governs_which_is_the_lower_dpi() {
        let large = dpi((1_000, 1_000), (720.0, 720.0));
        let small = dpi((1_000, 1_000), (72.0, 72.0));

        assert_eq!(large.governed_by(small), large);
        assert_eq!(small.governed_by(large), large, "order must not matter");
    }

    /// Each axis is governed by whichever placement is largest *along that
    /// axis*, which need not be the same placement.
    #[test]
    fn each_axis_is_governed_on_its_own() {
        let wide = dpi((300, 300), (300.0, 75.0));
        let tall = dpi((300, 300), (75.0, 300.0));

        assert_eq!(
            wide.governed_by(tall),
            EffectiveDpi {
                horizontal: 72.0,
                vertical: 72.0
            }
        );
    }

    /// A `cm` that collapses an axis paints nothing. Measured as a division
    /// it would be infinite DPI — and a minimum keeps an infinity happily,
    /// so an invisible placement would be allowed to govern a real one.
    #[test]
    fn a_placement_that_draws_nothing_cannot_be_measured() {
        assert_eq!(EffectiveDpi::measure((300, 300), (0.0, 150.0)), None);
        assert_eq!(EffectiveDpi::measure((300, 300), (150.0, 0.0)), None);
        assert_eq!(EffectiveDpi::measure((300, 300), (f64::NAN, 150.0)), None);
        assert_eq!(
            EffectiveDpi::measure((300, 300), (f64::INFINITY, 150.0)),
            None
        );
    }

    /// An image dictionary claiming zero samples along an axis is malformed,
    /// and there is no resolution to compute for it.
    #[test]
    fn an_image_with_no_samples_cannot_be_measured() {
        assert_eq!(EffectiveDpi::measure((0, 300), (150.0, 150.0)), None);
        assert_eq!(EffectiveDpi::measure((300, 0), (150.0, 150.0)), None);
    }
}
