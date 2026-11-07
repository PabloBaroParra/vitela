//! Which language the shell speaks, decided from the POSIX locale environment.
//!
//! [`resolve`] is pure — it is handed the four variables rather than reading
//! them — so the precedence rules can be pinned by a table test that does not
//! depend on the developer's own locale. [`from_env`] is the thin wrapper the
//! running application uses.

/// A language the shell has a translation for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Language {
    English,
    Spanish,
}

impl Language {
    /// The BCP 47 tag Fluent selects plural rules by.
    pub(super) fn tag(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Spanish => "es",
        }
    }
}

/// The language for a given set of locale variables, gettext-style.
///
/// The effective locale is the first non-empty of `LC_ALL`, `LC_MESSAGES`
/// and `LANG`. `LANGUAGE` — a colon-separated preference list — is consulted
/// first, unless the effective locale is `C`/`POSIX` (or unset, which is the
/// same thing): gettext ignores `LANGUAGE` there, and so does this.
pub(super) fn resolve(
    language: Option<&str>,
    lc_all: Option<&str>,
    lc_messages: Option<&str>,
    lang: Option<&str>,
) -> Language {
    let effective = [lc_all, lc_messages, lang]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty());
    let Some(effective) = effective else {
        return Language::English;
    };
    if is_c_locale(effective) {
        return Language::English;
    }
    let preferred = language
        .into_iter()
        .flat_map(|list| list.split(':'))
        .find_map(supported);
    preferred
        .or_else(|| supported(effective))
        .unwrap_or(Language::English)
}

/// `C`, `POSIX`, or a `C.<codeset>` variant such as `C.UTF-8`.
fn is_c_locale(value: &str) -> bool {
    let name = value.split(['.', '@']).next().unwrap_or_default();
    name == "C" || name == "POSIX"
}

/// The language a locale name or `LANGUAGE` entry asks for, if it is one we
/// have: `es`, `es_ES.UTF-8`, `es-419` and `es_AR@euro` are all Spanish.
fn supported(value: &str) -> Option<Language> {
    let primary = value.trim().split(['_', '-', '.', '@']).next()?;
    match primary.to_ascii_lowercase().as_str() {
        "es" => Some(Language::Spanish),
        "en" => Some(Language::English),
        _ => None,
    }
}

/// [`resolve`] applied to the process environment.
#[cfg_attr(test, allow(dead_code))]
pub(super) fn from_env() -> Language {
    let var = |name: &str| std::env::var(name).ok();
    resolve(
        var("LANGUAGE").as_deref(),
        var("LC_ALL").as_deref(),
        var("LC_MESSAGES").as_deref(),
        var("LANG").as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One locale variable at a time, as `LANG` — the common case.
    #[test]
    fn spanish_locales_in_any_spelling_resolve_to_spanish() {
        for value in [
            "es",
            "es_ES",
            "es_ES.UTF-8",
            "es_MX",
            "es-419",
            "es_AR.utf8@euro",
            "ES",
        ] {
            assert_eq!(
                resolve(None, None, None, Some(value)),
                Language::Spanish,
                "LANG={value}"
            );
        }
    }

    #[test]
    fn everything_else_resolves_to_english() {
        for value in [
            "",
            "C",
            "C.UTF-8",
            "POSIX",
            "en",
            "en_US.UTF-8",
            "en_GB",
            "fr_FR.UTF-8",
            "de",
            "esperanto",
            " ",
        ] {
            assert_eq!(
                resolve(None, None, None, Some(value)),
                Language::English,
                "LANG={value:?}"
            );
        }
        assert_eq!(resolve(None, None, None, None), Language::English);
    }

    #[test]
    fn lc_all_beats_lc_messages_beats_lang() {
        assert_eq!(
            resolve(
                None,
                Some("es_ES.UTF-8"),
                Some("en_US.UTF-8"),
                Some("en_US.UTF-8")
            ),
            Language::Spanish
        );
        assert_eq!(
            resolve(
                None,
                Some("en_US.UTF-8"),
                Some("es_ES.UTF-8"),
                Some("es_ES.UTF-8")
            ),
            Language::English
        );
        assert_eq!(
            resolve(None, None, Some("es_MX"), Some("en_US.UTF-8")),
            Language::Spanish
        );
        // An empty variable counts as unset, not as `C`.
        assert_eq!(
            resolve(None, Some(""), Some(""), Some("es_ES.UTF-8")),
            Language::Spanish
        );
    }

    #[test]
    fn language_list_is_consulted_before_the_locale() {
        assert_eq!(
            resolve(Some("es"), None, None, Some("en_US.UTF-8")),
            Language::Spanish
        );
        assert_eq!(
            resolve(Some("en"), None, None, Some("es_ES.UTF-8")),
            Language::English
        );
        // Unsupported entries are skipped; the first supported one wins.
        assert_eq!(
            resolve(Some("fr:de:es_ES:en"), None, None, Some("en_US.UTF-8")),
            Language::Spanish
        );
        // Nothing supported in the list falls through to the locale.
        assert_eq!(
            resolve(Some("fr:de"), None, None, Some("es_ES.UTF-8")),
            Language::Spanish
        );
        assert_eq!(
            resolve(Some(""), None, None, Some("es_ES.UTF-8")),
            Language::Spanish
        );
    }

    /// gettext ignores `LANGUAGE` when the locale is `C`, and so do we.
    #[test]
    fn a_c_locale_overrides_the_language_list() {
        for c in ["C", "C.UTF-8", "POSIX"] {
            assert_eq!(
                resolve(Some("es"), None, None, Some(c)),
                Language::English,
                "LANG={c}"
            );
            assert_eq!(
                resolve(Some("es"), Some(c), None, Some("es_ES.UTF-8")),
                Language::English,
                "LC_ALL={c}"
            );
        }
        // No locale at all is the C locale.
        assert_eq!(resolve(Some("es"), None, None, None), Language::English);
    }
}
