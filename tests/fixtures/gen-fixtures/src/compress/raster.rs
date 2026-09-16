//! The pixels the compression corpus is made of.
//!
//! Every raster here is *synthetic and deterministic* — the same bytes on
//! every machine and every run — because these fixtures are committed. A
//! photograph would say the same thing about the resampler and cost the
//! repository a megabyte per row; a run of `rand` would make a regenerated
//! fixture differ from the committed one for no reason anybody could act on.
//!
//! What the content has to be, though, is *not* noise. `pdf-compress`'s own
//! unit fixtures use noise on purpose (see its `test_fixtures::images`): a
//! flat fill flates to nothing whatever its sample count, so a unit test
//! about sample counts needs something incompressible behind it. A committed
//! corpus has the opposite constraint — noise is exactly what cannot be
//! stored cheaply — so these images are built the way a real page is: large
//! smooth areas with a little structure on them. That is both what keeps the
//! files small enough to commit and what makes them representative.

/// A scanned sheet of paper, as 8-bit grey samples.
///
/// Paper white with a faint tone gradient across the sheet, ruled with rows
/// of dark blocks standing in for lines of type. This is what a scanner
/// actually hands over, and it is the shape that makes a scan worth
/// compressing: almost all of it is one value, so JPEG stores it for very
/// little, and the file is nonetheless enormous because it is sampled at 200
/// dpi over a whole page.
pub(super) fn scanned_sheet(width: u32, height: u32) -> Vec<u8> {
    let mut samples = vec![0u8; (width as usize) * (height as usize)];

    let margin_x = width / 8;
    let margin_y = height / 12;
    let line_height = (height / 46).max(2);
    let ink_height = (line_height * 4 / 9).max(1);

    for y in 0..height {
        // The faint left-to-right fall-off a flatbed leaves behind. Small
        // amplitude, so it costs a JPEG almost nothing, but it means no two
        // columns are byte-identical and a resampler has something to average.
        let row = y as usize * width as usize;
        let within_line = y.saturating_sub(margin_y) % line_height;
        let is_text_row = y >= margin_y && y + margin_y < height && within_line < ink_height;

        for x in 0..width {
            let paper = 248 - u8::try_from((x * 6) / width.max(1)).unwrap_or(0);
            let value = if is_text_row && is_ink(x, y, width, margin_x, line_height) {
                48
            } else {
                paper
            };
            samples[row + x as usize] = value;
        }
    }

    samples
}

/// Whether the "type" on this row covers column `x`: words of varying width
/// separated by spaces, laid between the margins. Derived from the row index
/// so every line differs, which is what stops the whole page compressing
/// down to one repeated scanline.
fn is_ink(x: u32, y: u32, width: u32, margin_x: u32, line_height: u32) -> bool {
    if x < margin_x || x + margin_x >= width {
        return false;
    }

    let line = y / line_height.max(1);
    let column = x - margin_x;
    let word_width = 6 + (line.wrapping_mul(2_654_435_761) >> 28) % 10;
    let unit = (width / 64).max(1);
    let position = column / unit;

    // The last few words of a paragraph's final line stop short, the way a
    // real one does.
    if line % 7 == 6 && column * 3 > (width - 2 * margin_x) * 2 {
        return false;
    }

    position % (word_width + 2) < word_width
}

/// A textured field in `channels` components: a smooth two-axis gradient
/// with low-amplitude noise laid over it.
///
/// This is the shape of a photograph rather than of a diagram, and it is the
/// shape a *resampling* fixture has to have. Two earlier attempts at this
/// function are worth leaving written down, because both looked fine and
/// both made the corpus prove nothing:
///
/// - **a continuous gradient** stores badly — adjacent rows differ in every
///   byte, and a 600 x 600 RGB one lands at a quarter of a megabyte in a
///   committed repository;
/// - **flat tiles, or a quantised gradient**, store beautifully and then
///   *grow* when resampled. The resampler averages, so it puts every
///   intermediate value back that the quantisation took out, and the reduced
///   image is dearer to store than the original it replaces. `rewrite`
///   correctly threw the resample away, and the fixture reported
///   `images_skipped: 1` at every preset — a transparency row that never
///   exercised transparency.
///
/// Gradient-plus-noise is the shape whose stored size actually tracks its
/// sample count: a quarter of the samples is about a quarter of the bytes,
/// in both directions, so "resampling this made the file smaller" is true for
/// the reason the feature claims rather than by accident.
pub(super) fn textured(width: u32, height: u32, channels: usize) -> Vec<u8> {
    let mut samples = Vec::with_capacity((width as usize) * (height as usize) * channels);
    let mut state: u32 = 0x5eed_1234;

    for y in 0..height {
        for x in 0..width {
            // Small amplitude, so the gradient still reads as a gradient and
            // a JPEG still stores it cheaply — but enough that no two
            // neighbouring samples are equal.
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let noise = (state & 0x1F) as u8;

            for channel in 0..channels {
                let base = match channel {
                    0 => (x * 200) / width.max(1),
                    1 => (y * 200) / height.max(1),
                    _ => ((x + y) * 200) / (width + height).max(1),
                } as u8;
                samples.push(base.wrapping_add(noise));
            }
        }
    }

    samples
}

/// A soft mask: opaque in a large rounded region, transparent outside it,
/// with a genuine gradient across the boundary.
///
/// The gradient is the part that matters. A mask of nothing but 0 and 255 is
/// a mask a nearest-neighbour resampler reproduces exactly, and the corpus
/// would then hold a transparency fixture that could not tell a correct
/// implementation from a careless one.
pub(super) fn soft_mask(width: u32, height: u32) -> Vec<u8> {
    let mut samples = Vec::with_capacity((width as usize) * (height as usize));
    let (centre_x, centre_y) = (f64::from(width) / 2.0, f64::from(height) / 2.0);
    let radius = centre_x.min(centre_y);
    let feather = radius / 4.0;

    for y in 0..height {
        for x in 0..width {
            let dx = f64::from(x) - centre_x;
            let dy = f64::from(y) - centre_y;
            let distance = dx.hypot(dy);
            let alpha = ((radius - distance) / feather).clamp(0.0, 1.0);
            samples.push((alpha * 255.0).round() as u8);
        }
    }

    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scanned_sheet_is_mostly_paper() {
        let sheet = scanned_sheet(400, 500);
        let ink = sheet.iter().filter(|&&sample| sample < 128).count();

        assert!(
            ink * 4 < sheet.len(),
            "a page of type is not a quarter ink: {ink} of {} samples",
            sheet.len()
        );
        assert!(ink > 0, "the sheet has no type on it at all");
    }

    /// The property the whole corpus rests on: regenerating a committed
    /// fixture must produce the same file, or the next person to run the
    /// generator has a diff they cannot explain.
    #[test]
    fn every_raster_is_deterministic() {
        assert_eq!(scanned_sheet(64, 80), scanned_sheet(64, 80));
        assert_eq!(textured(32, 32, 3), textured(32, 32, 3));
        assert_eq!(soft_mask(32, 32), soft_mask(32, 32));
    }

    #[test]
    fn a_soft_mask_is_not_only_opaque_and_transparent() {
        let mask = soft_mask(128, 128);

        assert!(mask.contains(&255), "nothing is opaque");
        assert!(mask.contains(&0), "nothing is clear");
        assert!(
            mask.iter().any(|&alpha| (1..255).contains(&alpha)),
            "the mask has no gradient, so it cannot tell two resamplers apart"
        );
    }
}
