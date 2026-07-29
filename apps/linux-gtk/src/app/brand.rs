//! The application mark: the isotype shown in the page area while no document
//! is on screen, mirroring the empty state of the WinUI shell.

use gtk::gdk::Texture;
use gtk::gdk_pixbuf::prelude::PixbufLoaderExt;
use gtk::gdk_pixbuf::PixbufLoader;
use gtk::prelude::*;
use gtk::{Picture, Settings};

/// The two authored variants of the mark, linked in from the same shared
/// `assets/brand/` files the Windows shell copies beside its executable.
///
/// Two files rather than one tinted at runtime, for the same reason Windows
/// keeps two: the navy reads on a light theme and disappears on a dark one,
/// and the paths carry a literal fill rather than `currentColor`.
const MARK_LIGHT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/brand/vitela-app-mark.svg"
));
const MARK_DARK: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/brand/vitela-app-mark-dark.svg"
));

/// Logical edge of the mark, matching the 96px `Image` in the WinUI empty
/// state so both shells show it at the same size.
const MARK_LOGICAL_PX: i32 = 96;

/// Builds the mark widget and keeps it in step with the theme and the monitor
/// it is displayed on. The caller owns visibility: the mark is shown while the
/// page area is empty and hidden once a document has pages.
pub(crate) fn build_app_mark() -> Picture {
    let picture = Picture::new();
    picture.set_can_shrink(true);
    picture.set_halign(gtk::Align::Center);
    picture.set_valign(gtk::Align::Center);
    // The mark sits in an overlay above the scroller. Without this it would
    // become the event target over its own 96px square and swallow scrolls
    // that belong to the view underneath.
    picture.set_can_target(false);
    // A texture has no intrinsic logical size, only pixels — pin the widget to
    // the logical edge so a HiDPI render stays 96pt wide instead of 96 * scale.
    picture.set_size_request(MARK_LOGICAL_PX, MARK_LOGICAL_PX);
    draw_mark(&picture);

    // Re-rasterise when the answer to either input changes: the theme decides
    // which variant, the scale factor decides at what pixel size.
    if let Some(settings) = Settings::default() {
        for property in [
            "gtk-application-prefer-dark-theme",
            "gtk-theme-name",
            "gtk-icon-theme-name",
        ] {
            let weak = picture.downgrade();
            settings.connect_notify_local(Some(property), move |_, _| {
                if let Some(picture) = weak.upgrade() {
                    draw_mark(&picture);
                }
            });
        }
    }
    let weak = picture.downgrade();
    picture.connect_notify_local(Some("scale-factor"), move |_, _| {
        if let Some(picture) = weak.upgrade() {
            draw_mark(&picture);
        }
    });

    picture
}

fn draw_mark(picture: &Picture) {
    let edge = MARK_LOGICAL_PX * picture.scale_factor().max(1);
    let variant = if prefers_dark() {
        MARK_DARK
    } else {
        MARK_LIGHT
    };
    picture.set_paintable(rasterize(variant, edge).as_ref());
}

/// Rasterises the mark at `edge` physical pixels.
///
/// GDK's own texture loaders cover PNG, JPEG and TIFF only, so the SVG goes
/// through gdk-pixbuf's loader (librsvg). Sizing the loader before writing is
/// explicitly supported and makes librsvg render the vector *at* that size
/// rather than scaling a natural-size raster afterwards.
///
/// Returns `None` when the loader is unavailable — a desktop without the SVG
/// pixbuf loader installed shows an empty state without the mark rather than
/// failing to start.
fn rasterize(svg: &[u8], edge: i32) -> Option<Texture> {
    let loader = PixbufLoader::with_type("svg").ok()?;
    loader.set_size(edge, edge);
    loader.write(svg).ok()?;
    loader.close().ok()?;
    Some(Texture::for_pixbuf(&loader.pixbuf()?))
}

/// Whether the current theme is a dark one.
///
/// Plain GTK4 has no equivalent of libadwaita's `AdwStyleManager`, so this
/// reads the two settings that actually carry the preference: the portal maps
/// a dark colour scheme onto `gtk-application-prefer-dark-theme`, while a user
/// who picked a dark theme outright gets it in the theme name.
fn prefers_dark() -> bool {
    let Some(settings) = Settings::default() else {
        return false;
    };
    settings.is_gtk_application_prefer_dark_theme()
        || settings
            .gtk_theme_name()
            .is_some_and(|name| name.to_lowercase().contains("dark"))
}
