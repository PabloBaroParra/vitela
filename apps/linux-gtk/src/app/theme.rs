//! The shell's colour scheme: one palette, two values per colour, switched
//! live when the desktop's light/dark preference changes.
//!
//! ## Why named colours
//!
//! Every stylesheet in `app/` used to carry its own hex literals, so the app
//! was light no matter what the desktop asked for. Each sheet now names the
//! *role* a colour plays (`@vitela_surface`, `@vitela_text_muted`) and this
//! module is the only place that says what those roles look like. The
//! palette is emitted as `@define-color` lines ahead of the sheets, so
//! switching schemes is one provider reload — no selector changes.
//!
//! ## Why some roles share a value
//!
//! Tokens are per role, not per colour: the light palette has several greys
//! one shade apart that the dark palette collapses, and a light pixel is not
//! allowed to move to make the table shorter. Where two roles happen to hold
//! the same value in one scheme, they stay two tokens.
//!
//! ## What switches, and how
//!
//! * the CSS provider is reloaded with the other scheme's `@define-color`s;
//! * rasterised icons (tinted at raster time, so CSS cannot reach them) are
//!   redrawn through [`super::icons::retint_all`];
//! * the brand mark picks its own variant and listens to the same settings
//!   (see `brand`), so it is not driven from here.

use std::cell::{Cell, RefCell};

use gtk::prelude::*;
use gtk::{style_context_add_provider_for_display, CssProvider, Settings};

/// Which half of the palette is live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scheme {
    Light,
    Dark,
}

/// One role in the palette.
struct Token {
    name: &'static str,
    light: &'static str,
    dark: &'static str,
}

impl Token {
    fn value(&self, scheme: Scheme) -> &'static str {
        match scheme {
            Scheme::Light => self.light,
            Scheme::Dark => self.dark,
        }
    }
}

const fn token(name: &'static str, light: &'static str, dark: &'static str) -> Token {
    Token { name, light, dark }
}

/// The whole palette. Light values are the shell's original hex literals,
/// unchanged; dark values derive from Obsidian's dark theme.
///
/// `vitela_accent` is the accent as *text, icon or outline* and lightens on
/// dark so it still reads; `vitela_accent_fill` is the accent as a *filled
/// background under white text*, which is already dark enough and so does
/// not move. They share a light value on purpose.
const PALETTE: &[Token] = &[
    // Surfaces.
    token("vitela_window", "#f8f7fb", "#232323"),
    token("vitela_surface", "#ffffff", "#282828"),
    token("vitela_input", "#fcfbfe", "#2e2e2e"),
    token("vitela_card_hover", "#faf9fe", "#2e2e2e"),
    token("vitela_tile", "#f6f4fd", "#2e2e2e"),
    token("vitela_tile_disabled", "#f5f4f7", "#2e2e2e"),
    token("vitela_recessed", "#f2f0f5", "#212121"),
    token("vitela_canvas", "#e9e6ec", "#1c1c1c"),
    token("vitela_sash", "#e9e6ec", "#333333"),
    token("vitela_sheet", "#f1eef6", "#2e2e2e"),
    token("vitela_hover", "#f5f3fa", "#333333"),
    // Accent-tinted surfaces and borders.
    token("vitela_accent_soft", "#eee9fa", "#373144"),
    token("vitela_accent_soft_border", "#e7e2fb", "#373144"),
    token("vitela_checked", "#f2edff", "#3d364f"),
    token("vitela_accent_border", "#ded3ff", "#58497b"),
    // Borders.
    token("vitela_border", "#e3e0e9", "#333333"),
    token("vitela_border_indicator", "#e7e4ed", "#333333"),
    token("vitela_divider", "#eeecf2", "#333333"),
    token("vitela_border_disabled", "#eae8ef", "#333333"),
    token("vitela_input_border", "#ded9e9", "#3f3f3f"),
    token("vitela_border_strong", "#c9c2e0", "#555555"),
    // Text.
    token("vitela_text", "#302d3a", "#dadada"),
    token("vitela_text_secondary", "#51496a", "#b3b3b3"),
    token("vitela_text_muted", "#625b72", "#999999"),
    token("vitela_text_disabled", "#a49fb3", "#666666"),
    // Accent and status.
    token("vitela_accent", "#6b4eff", "#a882ff"),
    token("vitela_accent_fill", "#6b4eff", "#6b4eff"),
    token("vitela_accent_fill_deep", "#5a3ee6", "#5a3ee6"),
    token("vitela_on_accent", "#ffffff", "#ffffff"),
    token("vitela_success", "#1f8a4c", "#44cf6e"),
    // A tool accent that is too dim on the dark surfaces to keep.
    token("vitela_tool_protect", "#6366f1", "#818cf8"),
];

/// Icon tints (see `icons`) that follow the scheme, by the light hex the
/// caller names them with. Anything not listed — the other tool accents — is
/// readable on both and passes through unchanged.
const ICON_TINTS: &[(&str, &str)] = &[
    ("#6b4eff", "vitela_accent"),
    ("#51496a", "vitela_text_secondary"),
    ("#a49fb3", "vitela_text_disabled"),
    ("#6366f1", "vitela_tool_protect"),
];

fn lookup(name: &str) -> Option<&'static Token> {
    PALETTE.iter().find(|token| token.name == name)
}

/// The `@define-color` block for `scheme`.
fn palette_css(scheme: Scheme) -> String {
    PALETTE
        .iter()
        .map(|token| format!("@define-color {} {};\n", token.name, token.value(scheme)))
        .collect()
}

/// `sheets` preceded by the palette for `scheme`.
fn css_for(scheme: Scheme, sheets: &str) -> String {
    format!("{}{sheets}", palette_css(scheme))
}

/// The colour an icon authored with the light tint `light_hex` is drawn in
/// under `scheme`.
pub(crate) fn tint_for(light_hex: &str, scheme: Scheme) -> String {
    if scheme == Scheme::Dark {
        let mapped = ICON_TINTS
            .iter()
            .find(|(light, _)| light.eq_ignore_ascii_case(light_hex))
            .and_then(|(_, name)| lookup(name));
        if let Some(token) = mapped {
            return token.dark.to_string();
        }
    }
    light_hex.to_string()
}

/// [`tint_for`] under the live scheme.
pub(crate) fn current_tint(light_hex: &str) -> String {
    tint_for(light_hex, current_scheme())
}

/// The pure half of [`prefers_dark`].
fn scheme_for(prefer_dark_setting: bool, theme_name: Option<&str>) -> Scheme {
    let named_dark = theme_name.is_some_and(|name| name.to_lowercase().contains("dark"));
    if prefer_dark_setting || named_dark {
        Scheme::Dark
    } else {
        Scheme::Light
    }
}

/// The scheme the desktop is asking for right now.
///
/// Plain GTK4 has no equivalent of libadwaita's `AdwStyleManager`, so this
/// reads the two settings that actually carry the preference: the portal maps
/// a dark colour scheme onto `gtk-application-prefer-dark-theme`, while a user
/// who picked a dark theme outright gets it in the theme name.
fn desktop_scheme() -> Scheme {
    let Some(settings) = Settings::default() else {
        return Scheme::Light;
    };
    scheme_for(
        settings.is_gtk_application_prefer_dark_theme(),
        settings.gtk_theme_name().as_deref(),
    )
}

/// Whether the current theme is a dark one.
pub(crate) fn prefers_dark() -> bool {
    desktop_scheme() == Scheme::Dark
}

thread_local! {
    static LIVE: Cell<Scheme> = const { Cell::new(Scheme::Light) };
    static PROVIDER: RefCell<Option<Installed>> = const { RefCell::new(None) };
}

/// The one provider and the sheet text it is built from, kept so a scheme
/// change can rebuild it.
struct Installed {
    provider: CssProvider,
    sheets: String,
}

/// The scheme the shell is currently painted in.
pub(crate) fn current_scheme() -> Scheme {
    LIVE.get()
}

/// Installs `sheets` under the desktop's current scheme and keeps it in step
/// with the desktop from then on. Calling it again only replaces the sheet
/// text: there is still one provider and one pair of settings listeners.
pub(crate) fn install(sheets: String) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let scheme = desktop_scheme();
    LIVE.set(scheme);

    let first_install = PROVIDER.with_borrow_mut(|slot| match slot {
        Some(installed) => {
            installed.sheets = sheets;
            installed
                .provider
                .load_from_data(&css_for(scheme, &installed.sheets));
            false
        }
        None => {
            // One provider for every sheet: they share a palette and a
            // cascade, and two providers at the same priority would leave
            // which one wins a question of registration order.
            let provider = CssProvider::new();
            provider.load_from_data(&css_for(scheme, &sheets));
            style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            *slot = Some(Installed { provider, sheets });
            true
        }
    });

    if first_install {
        watch_desktop();
    }
}

/// Re-applies the palette when either setting behind [`desktop_scheme`]
/// changes.
fn watch_desktop() {
    let Some(settings) = Settings::default() else {
        return;
    };
    for property in ["gtk-application-prefer-dark-theme", "gtk-theme-name"] {
        settings.connect_notify_local(Some(property), |_, _| refresh());
    }
}

/// Moves the shell to whatever [`desktop_scheme`] says now, if that differs
/// from what is painted.
fn refresh() {
    let scheme = desktop_scheme();
    if scheme == LIVE.get() {
        return;
    }
    LIVE.set(scheme);
    PROVIDER.with_borrow(|slot| {
        if let Some(installed) = slot {
            installed
                .provider
                .load_from_data(&css_for(scheme, &installed.sheets));
        }
    });
    super::icons::retint_all();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::shell::all_sheets;

    /// The shell's original light literals, role by role. The light palette
    /// is a contract — dark mode must not move one light pixel.
    const ORIGINAL_LIGHT: &[(&str, &str)] = &[
        ("vitela_window", "#f8f7fb"),
        ("vitela_surface", "#ffffff"),
        ("vitela_input", "#fcfbfe"),
        ("vitela_card_hover", "#faf9fe"),
        ("vitela_tile", "#f6f4fd"),
        ("vitela_tile_disabled", "#f5f4f7"),
        ("vitela_recessed", "#f2f0f5"),
        ("vitela_canvas", "#e9e6ec"),
        ("vitela_sash", "#e9e6ec"),
        ("vitela_sheet", "#f1eef6"),
        ("vitela_hover", "#f5f3fa"),
        ("vitela_accent_soft", "#eee9fa"),
        ("vitela_accent_soft_border", "#e7e2fb"),
        ("vitela_checked", "#f2edff"),
        ("vitela_accent_border", "#ded3ff"),
        ("vitela_border", "#e3e0e9"),
        ("vitela_border_indicator", "#e7e4ed"),
        ("vitela_divider", "#eeecf2"),
        ("vitela_border_disabled", "#eae8ef"),
        ("vitela_input_border", "#ded9e9"),
        ("vitela_border_strong", "#c9c2e0"),
        ("vitela_text", "#302d3a"),
        ("vitela_text_secondary", "#51496a"),
        ("vitela_text_muted", "#625b72"),
        ("vitela_text_disabled", "#a49fb3"),
        ("vitela_accent", "#6b4eff"),
        ("vitela_accent_fill", "#6b4eff"),
        ("vitela_accent_fill_deep", "#5a3ee6"),
        ("vitela_on_accent", "#ffffff"),
        ("vitela_success", "#1f8a4c"),
        ("vitela_tool_protect", "#6366f1"),
    ];

    /// Every `@vitela_*` the sheets reference.
    fn referenced_tokens(css: &str) -> Vec<String> {
        css.split("@vitela_")
            .skip(1)
            .map(|rest| {
                let end = rest
                    .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
                    .unwrap_or(rest.len());
                format!("vitela_{}", &rest[..end])
            })
            .collect()
    }

    #[test]
    fn the_light_palette_is_exactly_the_original_colours() {
        assert_eq!(PALETTE.len(), ORIGINAL_LIGHT.len());
        for (name, light) in ORIGINAL_LIGHT {
            let token = lookup(name).unwrap_or_else(|| panic!("{name} is not in the palette"));
            assert_eq!(token.light, *light, "{name} moved a light pixel");
        }
    }

    #[test]
    fn every_palette_value_is_a_lowercase_six_digit_hex() {
        for token in PALETTE {
            for value in [token.light, token.dark] {
                let digits = value.strip_prefix('#').unwrap_or("");
                assert!(
                    digits.len() == 6
                        && digits
                            .chars()
                            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
                    "{} has a malformed value {value}",
                    token.name
                );
            }
        }
    }

    #[test]
    fn palette_names_are_unique() {
        for (index, token) in PALETTE.iter().enumerate() {
            assert!(
                PALETTE[index + 1..]
                    .iter()
                    .all(|other| other.name != token.name),
                "{} is defined twice",
                token.name
            );
        }
    }

    #[test]
    fn every_token_the_sheets_reference_is_defined_in_both_palettes() {
        let sheets = all_sheets();
        let referenced = referenced_tokens(&sheets);
        assert!(!referenced.is_empty(), "the sheets name no palette colours");
        for scheme in [Scheme::Light, Scheme::Dark] {
            let defined = palette_css(scheme);
            for name in &referenced {
                assert!(
                    defined.contains(&format!("@define-color {name} #")),
                    "{name} is referenced but not defined for {scheme:?}"
                );
            }
        }
    }

    #[test]
    fn the_sheets_carry_no_hardcoded_colours() {
        let sheets = all_sheets();
        for (index, _) in sheets.match_indices('#') {
            let digits: String = sheets[index + 1..]
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .collect();
            assert!(
                !(digits.len() == 3 || digits.len() == 6 || digits.len() == 8),
                "a sheet still hardcodes #{digits}"
            );
        }
        assert!(!sheets.contains("rgb("), "a sheet still hardcodes rgb()");
        assert!(!sheets.contains("rgba("), "a sheet still hardcodes rgba()");
    }

    #[test]
    fn the_dark_palette_is_the_approved_one() {
        for (name, dark) in [
            ("vitela_window", "#232323"),
            ("vitela_surface", "#282828"),
            ("vitela_input", "#2e2e2e"),
            ("vitela_tile", "#2e2e2e"),
            ("vitela_recessed", "#212121"),
            ("vitela_canvas", "#1c1c1c"),
            ("vitela_border", "#333333"),
            ("vitela_border_strong", "#555555"),
            ("vitela_input_border", "#3f3f3f"),
            ("vitela_hover", "#333333"),
            ("vitela_text", "#dadada"),
            ("vitela_text_secondary", "#b3b3b3"),
            ("vitela_text_muted", "#999999"),
            ("vitela_text_disabled", "#666666"),
            ("vitela_accent", "#a882ff"),
            ("vitela_accent_fill", "#6b4eff"),
            ("vitela_accent_fill_deep", "#5a3ee6"),
            ("vitela_accent_soft", "#373144"),
            ("vitela_checked", "#3d364f"),
            ("vitela_accent_border", "#58497b"),
            ("vitela_success", "#44cf6e"),
        ] {
            assert_eq!(lookup(name).expect(name).dark, dark, "{name}");
        }
    }

    #[test]
    fn a_palette_block_defines_each_token_with_the_scheme_value() {
        let light = palette_css(Scheme::Light);
        let dark = palette_css(Scheme::Dark);
        assert!(light.contains("@define-color vitela_window #f8f7fb;"));
        assert!(dark.contains("@define-color vitela_window #232323;"));
        assert!(css_for(Scheme::Dark, ".x {}").ends_with(".x {}"));
    }

    #[test]
    fn dark_is_chosen_by_the_preference_or_by_a_dark_theme_name() {
        assert_eq!(scheme_for(false, None), Scheme::Light);
        assert_eq!(scheme_for(false, Some("Adwaita")), Scheme::Light);
        assert_eq!(scheme_for(true, Some("Adwaita")), Scheme::Dark);
        assert_eq!(scheme_for(false, Some("Adwaita-dark")), Scheme::Dark);
        assert_eq!(scheme_for(false, Some("Yaru-DARK")), Scheme::Dark);
    }

    #[test]
    fn icon_tints_follow_the_scheme_only_where_they_must() {
        use crate::app::icons::{
            ACCENT_TINT, ANNOTATE_TINT, COMPRESS_TINT, MUTED_TINT, NEUTRAL_TINT, ORGANIZE_TINT,
            PROTECT_TINT, SIGN_TINT,
        };
        for tint in [
            ACCENT_TINT,
            NEUTRAL_TINT,
            MUTED_TINT,
            PROTECT_TINT,
            ANNOTATE_TINT,
            SIGN_TINT,
            ORGANIZE_TINT,
            COMPRESS_TINT,
        ] {
            assert_eq!(tint_for(tint, Scheme::Light), tint, "light never remaps");
        }
        assert_eq!(tint_for(ACCENT_TINT, Scheme::Dark), "#a882ff");
        assert_eq!(tint_for(NEUTRAL_TINT, Scheme::Dark), "#b3b3b3");
        assert_eq!(tint_for(MUTED_TINT, Scheme::Dark), "#666666");
        assert_eq!(tint_for(PROTECT_TINT, Scheme::Dark), "#818cf8");
        for tint in [ANNOTATE_TINT, SIGN_TINT, ORGANIZE_TINT, COMPRESS_TINT] {
            assert_eq!(tint_for(tint, Scheme::Dark), tint);
        }
    }

    #[test]
    fn every_remapped_icon_tint_names_a_real_token_with_a_matching_light_value() {
        for (light, name) in ICON_TINTS {
            let token = lookup(name).unwrap_or_else(|| panic!("{name} is not in the palette"));
            assert_eq!(token.light, *light);
        }
    }
}
