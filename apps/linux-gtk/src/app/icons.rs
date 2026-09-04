//! The shell's own icon set: SVGs compiled into the binary and tinted as they
//! are rasterised.
//!
//! ## Why not the icon theme
//!
//! `shell`'s module docs state the rule this module had to work within: no
//! `Image::from_icon_name` anywhere in `app/`. The application ships as an
//! AppImage and a `.deb` (T-053) that must look the same on a host with a
//! different icon theme, an incomplete one, or none at all — and a theme
//! lookup that misses leaves a blank button with no error anyone sees.
//!
//! Shipping the icons removes the question. These are hand-authored in
//! `assets/icons/`, in the same line style as the reference design, and go
//! through the librsvg pipeline `brand` already uses for the application
//! mark.
//!
//! ## Why the colour is substituted rather than inherited
//!
//! `assets/README.md` records why the brand mark carries a literal fill
//! rather than `currentColor`: neither shell's SVG pipeline resolves it —
//! librsvg has no CSS context to resolve it against, and would paint black.
//! The same applies here, but an icon set needs one shape in several colours
//! (the accent for a tool, the muted grey for a disabled one), and a file per
//! colour would be the same drawing maintained six times.
//!
//! So every authored icon carries [`TINT_TOKEN`] exactly once, on the one
//! `<g>` that owns its strokes, and [`tinted`] swaps it for the colour the
//! caller wants. The files stay valid, previewable SVGs; the substitution is
//! one documented string replace with a test behind it.

use gtk::gdk::Texture;
use gtk::gdk_pixbuf::prelude::PixbufLoaderExt;
use gtk::gdk_pixbuf::{Pixbuf, PixbufLoader};
use gtk::prelude::*;
use gtk::Image;

/// The colour literal every authored icon carries where its tint goes.
///
/// Black, so an icon opened outside this application still renders as the
/// drawing it is rather than as an invisible one.
const TINT_TOKEN: &str = "#000000";

/// The shell's own accent — the same `#6b4eff` the CSS uses for a primary
/// button and the active rail section. Worn by anything that is *this
/// application acting*, rather than one tool among several.
pub(crate) const ACCENT_TINT: &str = "#6b4eff";

/// The per-tool accents, taken from the reference design. Each names a tool
/// rather than a hue, so a palette change happens here and nowhere else.
///
/// Edit is the accent itself, as it is in the reference design: it is the
/// tool the application is named for.
pub(crate) const EDIT_TINT: &str = ACCENT_TINT;
pub(crate) const ANNOTATE_TINT: &str = "#14b8a6";
pub(crate) const SIGN_TINT: &str = "#ec4899";
pub(crate) const ORGANIZE_TINT: &str = "#22c55e";
pub(crate) const COMPRESS_TINT: &str = "#f59e0b";
pub(crate) const PROTECT_TINT: &str = "#6366f1";

/// Navigation and quick actions: the same secondary text colour their labels
/// use. They are a list to read, not a grid of features to pick from, and
/// six accent colours down the rail would compete with the one thing on it
/// that is actually highlighted — the section you are in.
pub(crate) const NEUTRAL_TINT: &str = "#51496a";

/// Anything disabled, matching the `:disabled` label colour in the shell's
/// CSS. A control with no feature behind it must not be the most colourful
/// thing on the card.
pub(crate) const MUTED_TINT: &str = "#a49fb3";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Icon {
    Home,
    Recent,
    Files,
    Edit,
    Annotate,
    Sign,
    Organize,
    Compress,
    Protect,
    NewFile,
    Sample,
    Delete,
}

/// Every icon, for the test that checks the whole set at once rather than
/// whichever one someone remembered to add a case for.
#[cfg(test)]
const ALL_ICONS: [Icon; 12] = [
    Icon::Home,
    Icon::Recent,
    Icon::Files,
    Icon::Edit,
    Icon::Annotate,
    Icon::Sign,
    Icon::Organize,
    Icon::Compress,
    Icon::Protect,
    Icon::NewFile,
    Icon::Sample,
    Icon::Delete,
];

macro_rules! icon_source {
    ($file:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/icons/",
            $file
        ))
    };
}

impl Icon {
    fn source(self) -> &'static str {
        match self {
            Icon::Home => icon_source!("home.svg"),
            Icon::Recent => icon_source!("recent.svg"),
            Icon::Files => icon_source!("files.svg"),
            Icon::Edit => icon_source!("edit.svg"),
            Icon::Annotate => icon_source!("annotate.svg"),
            Icon::Sign => icon_source!("sign.svg"),
            Icon::Organize => icon_source!("organize.svg"),
            Icon::Compress => icon_source!("compress.svg"),
            Icon::Protect => icon_source!("protect.svg"),
            Icon::NewFile => icon_source!("new-file.svg"),
            Icon::Sample => icon_source!("sample.svg"),
            Icon::Delete => icon_source!("delete.svg"),
        }
    }
}

/// Builds an icon widget pinned to `logical_edge` and painted in `color`.
///
/// ## Why `GtkImage` and not `GtkPicture`
///
/// A `GtkPicture` keeps its paintable's aspect ratio, which means it answers
/// **height-for-width**: ask a square one how tall it is at 70px wide and it
/// says 70. Icons live in boxes next to text, and a vertical `GtkBox` measures
/// its children against the box's own width — which is the width of the widest
/// child, the label. So in the Home tool grid the icon beside a 70px-wide
/// "Annotate" claimed 70px of height, centred its 24px drawing inside that,
/// and pushed the label to the bottom of the tile, while the icon beside a
/// 30px-wide "Edit" claimed 30 and looked almost right. `set_size_request` is
/// a floor, not a ceiling, so it does not stop this.
///
/// `GtkImage` requests exactly `pixel_size` in both directions and never grows
/// with the width it is offered. That is the whole difference, and it is the
/// reason every tile's icon and label now sit at the same height.
///
/// Note for anyone scanning for rule violations: `shell`'s module doc forbids
/// `Image::from_icon_name` — the *icon-theme lookup*. This builds an empty
/// `Image` and hands it a paintable we rasterised ourselves, which asks the
/// desktop for nothing.
///
/// Re-rasterises when the monitor scale changes, for the reason
/// `brand::build_mark` does: a texture has no logical size of its own, so a
/// bitmap made for a 1x display is drawn upscaled and soft on a 2x one.
/// Unlike the mark it ignores the *theme*, because the colour is the
/// caller's decision here, not the desktop's.
pub(crate) fn build_icon(icon: Icon, logical_edge: i32, color: &str) -> Image {
    let image = Image::new();
    image.set_pixel_size(logical_edge);
    image.set_halign(gtk::Align::Center);
    image.set_valign(gtk::Align::Center);
    // Decorative: every icon in this shell sits beside its own text label, so
    // announcing it as well would read the same word twice.
    image.set_can_target(false);
    image.update_property(&[gtk::accessible::Property::Label("")]);

    let color = color.to_string();
    draw_icon(&image, icon, logical_edge, &color);

    let weak = image.downgrade();
    image.connect_notify_local(Some("scale-factor"), move |_, _| {
        if let Some(image) = weak.upgrade() {
            draw_icon(&image, icon, logical_edge, &color);
        }
    });

    image
}

fn draw_icon(image: &Image, icon: Icon, logical_edge: i32, color: &str) {
    let edge = logical_edge * image.scale_factor().max(1);
    let svg = tinted(icon.source(), color);
    image.set_paintable(rasterize(svg.as_bytes(), edge).as_ref());
}

/// `source` with its tint token replaced by `color`.
fn tinted(source: &str, color: &str) -> String {
    source.replace(TINT_TOKEN, color)
}

/// Rasterises an SVG at `edge` physical pixels.
///
/// GDK's own texture loaders cover PNG, JPEG and TIFF only, so the SVG goes
/// through gdk-pixbuf's loader (librsvg). Sizing the loader before writing is
/// explicitly supported and makes librsvg render the vector *at* that size
/// rather than scaling a natural-size raster afterwards.
///
/// Returns `None` when the loader is unavailable — a desktop without the SVG
/// pixbuf loader installed draws the labels without their icons rather than
/// failing to start.
///
/// `pub(super)`: `brand` rasterises the application mark through this same
/// function. It used to carry its own copy; there is one SVG pipeline in this
/// shell, and it is this one.
pub(super) fn rasterize(svg: &[u8], edge: i32) -> Option<Texture> {
    Some(Texture::for_pixbuf(&rasterize_pixbuf(svg, edge)?))
}

/// The step before [`rasterize`]'s upload. Split out so the optical-grid test
/// can read the drawing back pixel by pixel — a `GdkTexture` is write-only
/// from here, a `GdkPixbuf` is not.
fn rasterize_pixbuf(svg: &[u8], edge: i32) -> Option<Pixbuf> {
    let loader = PixbufLoader::with_type("svg").ok()?;
    loader.set_size(edge, edge);
    loader.write(svg).ok()?;
    loader.close().ok()?;
    loader.pixbuf()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract every authored icon has to hold up: exactly one tint
    /// token, so [`tinted`] colours the whole drawing and nothing else.
    ///
    /// Zero occurrences would paint a black icon on every card with no error;
    /// two would mean a shape was authored with its own literal colour and
    /// would only show up as one stroke staying black. Neither is visible in
    /// a diff, so it is checked here for the whole set at once.
    #[test]
    fn every_icon_carries_exactly_one_tint_token() {
        for icon in ALL_ICONS {
            assert_eq!(
                icon.source().matches(TINT_TOKEN).count(),
                1,
                "{icon:?} must carry the tint token exactly once"
            );
        }
    }

    /// Tinting replaces the token and leaves the drawing alone.
    #[test]
    fn tinting_swaps_the_token_for_the_requested_colour() {
        let tinted_source = tinted(Icon::Sign.source(), SIGN_TINT);

        assert!(tinted_source.contains(SIGN_TINT));
        assert!(!tinted_source.contains(TINT_TOKEN));
        assert_eq!(
            tinted_source.matches("<path").count(),
            Icon::Sign.source().matches("<path").count()
        );
    }

    /// The optical grid every icon is drawn on, measured at [`GRID_EDGE_PX`]:
    /// ink from row [`GRID_INK_TOP`] to row [`GRID_INK_BOTTOM`], which is
    /// y 3.5 to 20.5 of the 24-unit viewBox.
    const GRID_EDGE_PX: i32 = 96;
    const GRID_INK_TOP: i32 = 10;
    const GRID_INK_BOTTOM: i32 = 86;

    /// Antialiasing puts a fractional pixel at each end, and a librsvg
    /// version may round it the other way. One pixel of slack absorbs that
    /// and still fails the 16-pixel gap this test was written to catch.
    const GRID_TOLERANCE_PX: i32 = 1;

    /// The first and last rows of `icon` that have any ink in them, rendered
    /// at [`GRID_EDGE_PX`]. `bottom` is exclusive, like any half-open range.
    fn vertical_ink_bounds(icon: Icon) -> (i32, i32) {
        let svg = tinted(icon.source(), NEUTRAL_TINT);
        let pixbuf = rasterize_pixbuf(svg.as_bytes(), GRID_EDGE_PX)
            .unwrap_or_else(|| panic!("{icon:?} must rasterise"));
        assert!(
            pixbuf.has_alpha(),
            "{icon:?} must rasterise with an alpha channel"
        );
        let channels = pixbuf.n_channels() as usize;
        let rowstride = pixbuf.rowstride() as usize;
        let pixels = pixbuf.read_pixel_bytes();

        let mut bounds: Option<(i32, i32)> = None;
        for y in 0..pixbuf.height() {
            let row = &pixels[y as usize * rowstride..];
            let inked = (0..pixbuf.width()).any(|x| row[x as usize * channels + channels - 1] > 0);
            if inked {
                bounds = Some(match bounds {
                    Some((top, _)) => (top, y + 1),
                    None => (y, y + 1),
                });
            }
        }
        bounds.unwrap_or_else(|| panic!("{icon:?} rasterised to nothing at all"))
    }

    /// **Every icon occupies the same vertical band.**
    ///
    /// The regression this exists for was visible and confusing: the first
    /// set was authored shape by shape, so its ink ran from 60 to 80 pixels
    /// tall at this size and two icons sat three pixels off centre. Dropped
    /// into a grid of tiles — where the widget boxes are provably identical,
    /// `content` and `caption` measure the same for every tile — the icons
    /// still read as jumping up and down, and took the labels' apparent
    /// baseline with them.
    ///
    /// Widget geometry cannot catch this: as far as GTK is concerned a 24px
    /// `Picture` is a 24px `Picture` whatever is drawn inside it. Only the
    /// pixels know. So this measures them.
    #[gtk::test]
    fn gtk_ui_every_icon_is_drawn_on_the_same_optical_grid() {
        for icon in ALL_ICONS {
            let (top, bottom) = vertical_ink_bounds(icon);
            assert!(
                (top - GRID_INK_TOP).abs() <= GRID_TOLERANCE_PX,
                "{icon:?} ink starts at row {top}, off the shared grid's {GRID_INK_TOP}"
            );
            assert!(
                (bottom - GRID_INK_BOTTOM).abs() <= GRID_TOLERANCE_PX,
                "{icon:?} ink ends at row {bottom}, off the shared grid's {GRID_INK_BOTTOM}"
            );
        }
    }

    /// **An icon's height must not depend on how wide a row it is given.**
    ///
    /// This is the defect that made the Home tool grid look ragged, reduced
    /// to the one measurement that shows it. A vertical `GtkBox` measures its
    /// children against its own width — the width of the widest child, which
    /// is the label — so a widget that answers height-for-width grows
    /// vertically as the word beside it gets longer, taking the label under
    /// it down the tile with it. `GtkPicture` answers height-for-width;
    /// `GtkImage` with a `pixel_size` does not.
    ///
    /// The widths below are the real ones: 30 is "Edit", 74 is "Compress".
    /// Under `GtkPicture` this returned 30 and 74 instead of 24 and 24, which
    /// is exactly the 44px of drift that was on screen.
    #[gtk::test]
    fn gtk_ui_an_icon_keeps_its_height_however_wide_a_row_it_is_given() {
        let icon = build_icon(Icon::Sign, 24, NEUTRAL_TINT);

        for width in [-1, 24, 30, 74, 200] {
            let (minimum, natural, _, _) = icon.measure(gtk::Orientation::Vertical, width);
            assert_eq!(
                (minimum, natural),
                (24, 24),
                "the icon claimed a different height when offered {width}px of width"
            );
        }
    }

    /// Every icon has to survive the real librsvg pipeline at the two sizes
    /// the shell asks for. A malformed path would otherwise reach a user as a
    /// silently missing icon.
    #[gtk::test]
    fn gtk_ui_every_icon_rasterises_at_both_sizes() {
        for icon in ALL_ICONS {
            for edge in [16, 24] {
                let svg = tinted(icon.source(), NEUTRAL_TINT);
                let texture = rasterize(svg.as_bytes(), edge)
                    .unwrap_or_else(|| panic!("{icon:?} must rasterise at {edge}px"));
                assert_eq!(texture.width(), edge);
                assert_eq!(texture.height(), edge);
            }
        }
    }
}
