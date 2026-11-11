//! User-facing text, in the user's language.
//!
//! Strings live in Fluent files under `apps/linux-gtk/locales/`, embedded at
//! compile time. The language is chosen once per thread from the POSIX locale
//! environment ([`locale`]); GTK is single-threaded, so in practice that is
//! once per process.
//!
//! ```ignore
//! label.set_text(&tr("home-recent-title"));
//! meta.set_text(&tr_args("home-recent-page-count", &[("count", pages.into())]));
//! ```
//!
//! A message missing from the active language falls back to English, and one
//! missing from English too falls back to its own key — never a panic in a
//! release build. A debug build panics instead, so a typo in a key fails the
//! first test that renders it.
//!
//! Tests always run in English, whatever the developer's locale; a test that
//! needs Spanish takes a [`speak`] guard.
//!
//! Migrated surfaces: Home, navigation rail, editor toolbar, search, tools tabs,
//! document properties, the Edit PDF panel with its selection hints and mode
//! messages, and signing: certificate/card/token signing and the drawn
//! signature. Other feature panels, edit dialogs, document workflows and shared
//! permission/core error details are not yet localized.

mod bundle;
mod locale;

use std::cell::{Cell, OnceCell};

use fluent_bundle::{FluentBundle, FluentResource};

pub(crate) use fluent_bundle::FluentValue;
pub(crate) use locale::Language;

use bundle::Rendered;

thread_local! {
    static ACTIVE: Cell<Language> = Cell::new(initial_language());
    static ENGLISH: OnceCell<FluentBundle<FluentResource>> = const { OnceCell::new() };
    static SPANISH: OnceCell<FluentBundle<FluentResource>> = const { OnceCell::new() };
}

#[cfg(not(test))]
fn initial_language() -> Language {
    locale::from_env()
}

/// Deterministic tests: English, regardless of `LANG` on the machine.
#[cfg(test)]
fn initial_language() -> Language {
    Language::English
}

/// Message `key` in the active language.
pub(crate) fn tr(key: &str) -> String {
    tr_args(key, &[])
}

/// Message `key` in the active language, with Fluent arguments.
pub(crate) fn tr_args(key: &str, args: &[(&str, FluentValue<'_>)]) -> String {
    let active = ACTIVE.with(Cell::get);
    if let Rendered::Complete(text) =
        with_bundle(active, |bundle| bundle::render(bundle, key, args))
    {
        return text;
    }
    match with_bundle(Language::English, |bundle| {
        bundle::render(bundle, key, args)
    }) {
        Rendered::Complete(text) => text,
        Rendered::Partial(text) => {
            if cfg!(debug_assertions) {
                panic!("i18n: message {key:?} did not format cleanly: {text:?}");
            }
            text
        }
        Rendered::Missing => {
            if cfg!(debug_assertions) {
                panic!("i18n: no English message {key:?}");
            }
            key.to_owned()
        }
    }
}

/// Runs `f` against this thread's bundle for `language`, building it on
/// first use.
fn with_bundle<T>(language: Language, f: impl FnOnce(&FluentBundle<FluentResource>) -> T) -> T {
    let slot = match language {
        Language::English => &ENGLISH,
        Language::Spanish => &SPANISH,
    };
    slot.with(|cell| f(cell.get_or_init(|| bundle::build(language))))
}

/// Switches this thread to `language` until the guard drops.
///
/// A guard rather than a setter because `#[gtk::test]` cases share one
/// thread: a language left switched would leak into every later case.
#[cfg(test)]
pub(crate) fn speak(language: Language) -> LanguageGuard {
    let previous = ACTIVE.with(|active| active.replace(language));
    LanguageGuard { previous }
}

#[cfg(test)]
#[must_use = "the language reverts as soon as the guard drops"]
pub(crate) struct LanguageGuard {
    previous: Language,
}

#[cfg(test)]
impl Drop for LanguageGuard {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.set(self.previous));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_speak_english_by_default() {
        assert_eq!(tr("home-recent-title"), "Recent");
        assert_eq!(tr("home-welcome-title"), "Welcome to Vitela");
    }

    #[test]
    fn english_plurals_and_placeables() {
        assert_eq!(
            tr_args("home-recent-page-count", &[("count", 1.into())]),
            "1 page"
        );
        assert_eq!(
            tr_args("home-recent-page-count", &[("count", 12.into())]),
            "12 pages"
        );
        assert_eq!(
            tr_args("home-recent-remove-failed", &[("name", "a.pdf".into())]),
            "Could not remove a.pdf from Recent."
        );
    }

    #[test]
    fn spanish_plurals_and_placeables() {
        let _spanish = speak(Language::Spanish);
        assert_eq!(tr("home-recent-title"), "Recientes");
        assert_eq!(
            tr_args("home-recent-page-count", &[("count", 1.into())]),
            "1 página"
        );
        assert_eq!(
            tr_args("home-recent-page-count", &[("count", 0.into())]),
            "0 páginas"
        );
        assert_eq!(
            tr_args("home-recent-page-count", &[("count", 12.into())]),
            "12 páginas"
        );
        assert_eq!(
            tr_args("home-recent-opened-today", &[("time", "10:45".into())]),
            "Abierto hoy, 10:45"
        );
    }

    #[test]
    fn the_guard_restores_the_previous_language() {
        {
            let _spanish = speak(Language::Spanish);
            assert_eq!(tr("home-tools-title"), "Herramientas");
            {
                let _english = speak(Language::English);
                assert_eq!(tr("home-tools-title"), "Tools");
            }
            assert_eq!(tr("home-tools-title"), "Herramientas");
        }
        assert_eq!(tr("home-tools-title"), "Tools");
    }

    #[test]
    fn spanish_search_messages_keep_arguments_verbatim() {
        let _spanish = speak(Language::Spanish);
        for (key, expected) in [
            ("search-running", "Buscando «{PDF} á»..."),
            ("search-no-matches", "No hay coincidencias para «{PDF} á»."),
        ] {
            assert_eq!(tr_args(key, &[("query", "{PDF} á".into())]), expected);
        }
        assert_eq!(
            tr_args("search-failed", &[("error", "PDFium: {detail}".into())]),
            "No se pudo buscar: PDFium: {detail}"
        );
    }
}
