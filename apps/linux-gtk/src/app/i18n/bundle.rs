//! The translations themselves: embedded `.ftl` sources, one Fluent bundle
//! per language, and formatting a message out of one.
//!
//! The sources are compiled in with `include_str!`, so there is nothing to
//! install or find at run time and a packaged build cannot ship without them.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

use super::locale::Language;

const ENGLISH_FTL: &str = include_str!("../../../locales/en.ftl");
const SPANISH_FTL: &str = include_str!("../../../locales/es.ftl");

/// The `.ftl` source for `language`.
pub(super) fn source(language: Language) -> &'static str {
    match language {
        Language::English => ENGLISH_FTL,
        Language::Spanish => SPANISH_FTL,
    }
}

/// The bundle for `language`.
pub(super) fn build(language: Language) -> FluentBundle<FluentResource> {
    // A parse error still yields every entry that did parse; the parse test
    // keeps both files at zero errors, so this is a release-build safety net.
    let resource = match FluentResource::try_new(source(language).to_owned()) {
        Ok(resource) | Err((resource, _)) => resource,
    };
    let id: LanguageIdentifier = language.tag().parse().unwrap_or_default();
    let mut bundle = FluentBundle::new(vec![id]);
    // Without this Fluent wraps every placeable in U+2068/U+2069, which GTK
    // labels render as visible boxes on some fonts.
    bundle.set_use_isolating(false);
    bundle.add_resource_overriding(resource);
    bundle
}

/// What formatting a message produced.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Rendered {
    /// The message, fully formatted.
    Complete(String),
    /// The message, but Fluent reported errors (a missing argument, say);
    /// the text has `{$name}`-style holes where they happened.
    Partial(String),
    /// No message with that id, or one with no value.
    Missing,
}

/// Formats message `key` from `bundle` with `args`.
pub(super) fn render(
    bundle: &FluentBundle<FluentResource>,
    key: &str,
    args: &[(&str, FluentValue<'_>)],
) -> Rendered {
    let Some(pattern) = bundle.get_message(key).and_then(|message| message.value()) else {
        return Rendered::Missing;
    };
    let args = (!args.is_empty()).then(|| {
        let mut fluent_args = FluentArgs::new();
        for (name, value) in args {
            fluent_args.set(*name, value.clone());
        }
        fluent_args
    });
    let mut errors = Vec::new();
    let text = bundle
        .format_pattern(pattern, args.as_ref(), &mut errors)
        .into_owned();
    if errors.is_empty() {
        Rendered::Complete(text)
    } else {
        Rendered::Partial(text)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use fluent_syntax::ast;

    use super::*;

    const LANGUAGES: [Language; 2] = [Language::English, Language::Spanish];

    fn parse(language: Language) -> FluentResource {
        FluentResource::try_new(source(language).to_owned())
            .unwrap_or_else(|(_, errors)| panic!("{language:?} .ftl has parse errors: {errors:?}"))
    }

    /// Message id -> its attribute names, plus whether it has a value.
    fn shape(language: Language) -> BTreeMap<String, (bool, BTreeSet<String>)> {
        let resource = parse(language);
        let mut messages = BTreeMap::new();
        for entry in resource.entries() {
            if let ast::Entry::Message(message) = entry {
                let attributes = message
                    .attributes
                    .iter()
                    .map(|attribute| attribute.id.name.to_string())
                    .collect();
                messages.insert(
                    message.id.name.to_string(),
                    (message.value.is_some(), attributes),
                );
            }
        }
        messages
    }

    #[test]
    fn both_translations_parse_without_errors() {
        for language in LANGUAGES {
            let resource = parse(language);
            assert!(
                resource.entries().count() > 0,
                "{language:?} .ftl defines nothing"
            );
        }
    }

    /// Every id in one file exists in the other, with the same attributes —
    /// a key missing from Spanish would silently fall back to English.
    #[test]
    fn english_and_spanish_define_the_same_messages() {
        let english = shape(Language::English);
        let spanish = shape(Language::Spanish);

        let only_english: Vec<_> = english
            .keys()
            .filter(|id| !spanish.contains_key(*id))
            .collect();
        let only_spanish: Vec<_> = spanish
            .keys()
            .filter(|id| !english.contains_key(*id))
            .collect();
        assert!(
            only_english.is_empty(),
            "missing from es.ftl: {only_english:?}"
        );
        assert!(
            only_spanish.is_empty(),
            "missing from en.ftl: {only_spanish:?}"
        );
        assert_eq!(english, spanish, "message values/attributes differ");
        assert!(
            english.values().all(|(has_value, _)| *has_value),
            "every message is looked up by value, so each must have one"
        );
    }

    #[test]
    fn a_known_message_renders_and_an_unknown_one_is_missing() {
        let bundle = build(Language::English);
        assert_eq!(
            render(&bundle, "home-recent-title", &[]),
            Rendered::Complete("Recent".to_string())
        );
        assert_eq!(render(&bundle, "no-such-message", &[]), Rendered::Missing);
    }

    /// Placeables come out without the U+2068/U+2069 isolation marks Fluent
    /// adds by default — GTK labels would show them.
    #[test]
    fn placeables_are_not_wrapped_in_bidi_isolation_marks() {
        let bundle = build(Language::English);
        assert_eq!(
            render(
                &bundle,
                "home-recent-open-card",
                &[("name", "a.pdf".into())]
            ),
            Rendered::Complete("Open a.pdf".to_string())
        );
    }

    #[test]
    fn a_missing_argument_is_reported_as_partial() {
        let bundle = build(Language::English);
        assert!(matches!(
            render(&bundle, "home-recent-open-card", &[]),
            Rendered::Partial(_)
        ));
    }
}
