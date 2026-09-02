//! The application mark: the isotype shown in the page area while no document
//! is on screen, mirroring the empty state of the WinUI shell, and the brand
//! lockup (isotype + wordmark) the Home view's header and the app rail carry.

use gtk::prelude::*;
use gtk::{Box as GtkBox, Label, Orientation, Picture, Settings};

use super::icons::rasterize;

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

/// Logical edge of the mark inside [`build_brand_lockup`]. Sized against the
/// wordmark beside it rather than against the empty state's 96px: at anything
/// larger the isotype outweighs the word it is meant to sit with, and the app
/// rail — the narrowest place the lockup appears — has ~88px of usable width.
const LOCKUP_MARK_PX: i32 = 22;

/// Builds the mark widget and keeps it in step with the theme and the monitor
/// it is displayed on. The caller owns visibility: the mark is shown while the
/// page area is empty and hidden once a document has pages.
pub(crate) fn build_app_mark() -> Picture {
    let picture = build_mark(MARK_LOGICAL_PX);
    // The mark sits in an overlay above the scroller. Without this it would
    // become the event target over its own 96px square and swallow scrolls
    // that belong to the view underneath. Only this instance needs it — the
    // lockup's copy sits in an ordinary box with nothing behind it.
    picture.set_can_target(false);
    picture
}

/// The brand lockup: the isotype and the "Vitela" wordmark as one unit, for
/// the places that identify the *application* rather than stand in for a
/// missing document — the Home view's header and the app rail.
///
/// One widget rather than each caller pairing a `Picture` with a `Label`, so
/// the mark and the word cannot drift apart in spacing or alignment between
/// the two places they appear. Its accessible name covers the pair: the
/// wordmark already says "Vitela", and a screen reader announcing it twice
/// (once for the image, once for the label) is worse than once.
pub(crate) fn build_brand_lockup() -> GtkBox {
    let lockup = GtkBox::new(Orientation::Horizontal, 8);
    lockup.add_css_class("brand-lockup");
    lockup.set_valign(gtk::Align::Center);
    lockup.append(&build_mark(LOCKUP_MARK_PX));

    let wordmark = Label::new(Some("Vitela"));
    wordmark.set_xalign(0.0);
    wordmark.add_css_class("brand-word");
    lockup.append(&wordmark);

    lockup.update_property(&[gtk::accessible::Property::Label("Vitela")]);
    lockup
}

/// The shared body of [`build_app_mark`] and [`build_brand_lockup`]: a
/// `Picture` pinned to `edge` logical pixels that re-rasterises itself
/// whenever the theme or the monitor scale changes.
fn build_mark(edge: i32) -> Picture {
    let picture = Picture::new();
    picture.set_can_shrink(true);
    picture.set_halign(gtk::Align::Center);
    picture.set_valign(gtk::Align::Center);
    // A texture has no intrinsic logical size, only pixels — pin the widget to
    // the logical edge so a HiDPI render stays `edge` pt wide instead of
    // `edge * scale`.
    picture.set_size_request(edge, edge);
    draw_mark(&picture, edge);

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
                    draw_mark(&picture, edge);
                }
            });
        }
    }
    let weak = picture.downgrade();
    picture.connect_notify_local(Some("scale-factor"), move |_, _| {
        if let Some(picture) = weak.upgrade() {
            draw_mark(&picture, edge);
        }
    });

    picture
}

fn draw_mark(picture: &Picture, logical_edge: i32) {
    let edge = logical_edge * picture.scale_factor().max(1);
    let variant = if prefers_dark() {
        MARK_DARK
    } else {
        MARK_LIGHT
    };
    picture.set_paintable(rasterize(variant, edge).as_ref());
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The lockup is the one place the mark and the wordmark appear together,
    /// and the Home header and the app rail both take it as a unit — so the
    /// pair, and the single accessible name covering them, is the contract.
    #[gtk::test]
    fn gtk_ui_the_brand_lockup_pairs_the_mark_with_the_wordmark() {
        let lockup = build_brand_lockup();

        let children: Vec<_> =
            std::iter::successors(lockup.first_child(), |child| child.next_sibling()).collect();

        assert_eq!(children.len(), 2, "the lockup is the mark plus the word");
        assert!(children[0].downcast_ref::<Picture>().is_some());
        assert_eq!(
            children[1]
                .downcast_ref::<Label>()
                .map(|label| label.text().to_string())
                .as_deref(),
            Some("Vitela")
        );
    }

    /// The mark is pinned in *logical* pixels at both sizes it is built at —
    /// the reason `build_mark` takes an edge at all rather than each caller
    /// rasterising its own.
    #[gtk::test]
    fn gtk_ui_the_mark_is_pinned_to_its_logical_edge() {
        assert_eq!(
            build_app_mark().size_request(),
            (MARK_LOGICAL_PX, MARK_LOGICAL_PX)
        );
        assert_eq!(
            build_mark(LOCKUP_MARK_PX).size_request(),
            (LOCKUP_MARK_PX, LOCKUP_MARK_PX)
        );
    }
}
