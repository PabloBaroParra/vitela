using System.Text.RegularExpressions;
using Linguini.Syntax.Ast;
using Linguini.Syntax.Parser;
using Pdf.Windows.Localization;

/// <summary>
/// The shell's i18n contract, ported from the GTK shell's app/i18n tests:
/// language negotiation, per-message English fallback, Fluent arguments and
/// plurals, en/es parity, and every key the shell looks up existing in en.ftl.
/// </summary>
internal static class LocalizationTests
{
    private static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }

    private static void Equal(string actual, string expected, string what) =>
        Check(actual == expected, $"{what}: expected \"{expected}\", got \"{actual}\"");

    public static Task NegotiatesTheLanguageAsync()
    {
        foreach (var culture in new[] { "es", "es-ES", "es-MX", "es-419", "es_AR", "ES" })
            Check(LanguageNegotiation.Resolve(null, culture) == Language.Spanish, $"UI culture {culture} must resolve to Spanish");
        foreach (var culture in new[] { "", " ", "en", "en-US", "en-GB", "fr-FR", "de", "eo", "esperanto", null })
            Check(LanguageNegotiation.Resolve(null, culture) == Language.English, $"UI culture '{culture}' must resolve to English");

        Check(LanguageNegotiation.Resolve("es", "en-US") == Language.Spanish, "the override beats the UI culture");
        Check(LanguageNegotiation.Resolve("en", "es-ES") == Language.English, "an English override beats a Spanish UI culture");
        Check(LanguageNegotiation.Resolve("es-MX", "en-US") == Language.Spanish, "the override accepts a region");
        Check(LanguageNegotiation.Resolve("fr", "es-ES") == Language.Spanish, "an unsupported override falls through to the UI culture");
        Check(LanguageNegotiation.Resolve("", "es-ES") == Language.Spanish, "an empty override counts as unset");
        Check(LanguageNegotiation.Resolve("  ", "en-US") == Language.English, "a blank override counts as unset");
        // A colon-separated list, like Linux's LANGUAGE: the first supported entry wins.
        Check(LanguageNegotiation.Resolve("fr:es", "en-US") == Language.Spanish, "the first supported entry of an override list wins");
        Check(LanguageNegotiation.Resolve("en:es", "es-ES") == Language.English, "an earlier entry beats a later one");
        Check(LanguageNegotiation.Resolve(":es_MX:en", "en-US") == Language.Spanish, "empty entries in an override list are skipped");
        Check(LanguageNegotiation.Resolve("fr:de", "es-ES") == Language.Spanish, "a list with no supported entry falls through to the UI culture");
        Check(LanguageNegotiation.OverrideVariable == "VITELA_LANGUAGE", "the override variable is documented as VITELA_LANGUAGE");
        return Task.CompletedTask;
    }

    public static Task KeepsBothTranslationsInStepAsync()
    {
        var english = Shape(Language.English);
        var spanish = Shape(Language.Spanish);
        Check(english.Count > 0, "en.ftl defines nothing");
        var onlyEnglish = english.Keys.Except(spanish.Keys).ToArray();
        var onlySpanish = spanish.Keys.Except(english.Keys).ToArray();
        Check(onlyEnglish.Length == 0, "missing from es.ftl: " + string.Join(", ", onlyEnglish));
        Check(onlySpanish.Length == 0, "missing from en.ftl: " + string.Join(", ", onlySpanish));
        foreach (var (id, shape) in english)
        {
            Check(shape.HasValue, $"{id} has no value; every message is looked up by value");
            Check(shape.Attributes == spanish[id].Attributes, $"{id} has different attributes in en.ftl and es.ftl");
            Check(spanish[id].HasValue, $"{id} has no value in es.ftl");
        }
        return Task.CompletedTask;
    }

    /// <summary>A key the shell asks for that en.ftl lacks would render as the key itself.</summary>
    public static Task DefinesEveryKeyTheShellUsesAsync()
    {
        var defined = Shape(Language.English).Keys.ToHashSet(StringComparer.Ordinal);
        var shell = FindShellSources();
        var used = new SortedSet<string>(StringComparer.Ordinal);
        var code = new Regex(@"\bT\(\s*""([^""]+)""", RegexOptions.CultureInvariant);
        var xaml = new Regex(@"\{i18n:T\s+Key=([^\s,}]+)", RegexOptions.CultureInvariant);
        foreach (var file in Directory.EnumerateFiles(shell, "*.*", SearchOption.AllDirectories))
        {
            var relative = Path.GetRelativePath(shell, file);
            if (relative.StartsWith("bin" + Path.DirectorySeparatorChar) || relative.StartsWith("obj" + Path.DirectorySeparatorChar)) continue;
            var pattern = Path.GetExtension(file) switch { ".cs" => code, ".xaml" => xaml, _ => null };
            if (pattern is null) continue;
            foreach (Match match in pattern.Matches(File.ReadAllText(file))) used.Add(match.Groups[1].Value);
        }
        Check(used.Count > 0, "the scan found no localized strings at all; the patterns are stale");
        var missing = used.Where(key => !defined.Contains(key)).ToArray();
        Check(missing.Length == 0, "used by the shell but missing from en.ftl: " + string.Join(", ", missing));
        return Task.CompletedTask;
    }

    public static Task SpeaksEnglishByDefaultInTestsAsync()
    {
        Check(I18n.Active == Language.English, "tests must speak English whatever the machine's UI language");
        Equal(I18n.T("home-recent-title"), "Recent", "English Recent heading");
        Equal(I18n.T("home-welcome-title"), "Welcome to Vitela", "English welcome");
        return Task.CompletedTask;
    }

    public static Task FormatsArgumentsAndPluralsAsync()
    {
        Equal(I18n.T("home-recent-page-count", ("count", 1)), "1 page", "English singular");
        Equal(I18n.T("home-recent-page-count", ("count", 12)), "12 pages", "English plural");
        Equal(I18n.T("home-recent-remove-failed", ("name", "a.pdf")), "Could not remove a.pdf from Recent.", "English placeable");
        // No U+2068/U+2069 isolation marks: they show as boxes in some fonts.
        Equal(I18n.T("home-recent-open-card", ("name", "a.pdf")), "Open a.pdf", "placeables are not bidi-isolated");

        using (I18n.Speak(Language.Spanish))
        {
            Equal(I18n.T("home-recent-title"), "Recientes", "Spanish Recent heading");
            Equal(I18n.T("home-recent-page-count", ("count", 1)), "1 página", "Spanish singular");
            Equal(I18n.T("home-recent-page-count", ("count", 0)), "0 páginas", "Spanish zero");
            Equal(I18n.T("home-recent-page-count", ("count", 12)), "12 páginas", "Spanish plural");
            Equal(I18n.T("home-recent-opened-today", ("time", "10:45")), "Abierto hoy, 10:45", "Spanish placeable");
            Equal(I18n.T("search-no-matches", ("query", "{PDF} á")), "No hay coincidencias para «{PDF} á».", "arguments are kept verbatim");
            Equal(Pdf.Windows.Viewer.SearchSelection.Status("needle", 0, 3), "Coincidencia 1 de 3 para «needle».", "Spanish search status");
            Equal(I18n.T("search-result", ("page", 4u), ("text", "a.b")), "Página 4: a.b", "unsigned page numbers format as numbers");
            Equal(I18n.T("metadata-pages-value", ("count", 12u)), "Páginas: 12", "Spanish page count in properties");
        }
        return Task.CompletedTask;
    }

    public static Task RestoresTheLanguageWhenTheGuardEndsAsync()
    {
        using (I18n.Speak(Language.Spanish))
        {
            Equal(I18n.T("home-tools-title"), "Herramientas", "Spanish inside the guard");
            using (I18n.Speak(Language.English)) Equal(I18n.T("home-tools-title"), "Tools", "nested English guard");
            Equal(I18n.T("home-tools-title"), "Herramientas", "the nested guard restores Spanish");
        }
        Equal(I18n.T("home-tools-title"), "Tools", "the outer guard restores English");
        return Task.CompletedTask;
    }

    public static Task FallsBackPerMessageAsync()
    {
        var warnings = new List<string>();
        var translations = new Translations(Language.Spanish,
            english: "both = Both\nonly-english = Only English\nneeds = Hello { $name }\n",
            spanish: "both = Ambos\nneeds = Hola { $name }\n",
            warn: warnings.Add);
        Equal(translations.T("both"), "Ambos", "a Spanish message is used when present");
        Equal(translations.T("only-english"), "Only English", "a message missing from Spanish falls back to English");
        Check(warnings.Count == 0, "a per-language fallback is not a warning; the parity test guards it");

        Equal(translations.T("no-such-message"), "no-such-message", "a message missing everywhere renders as its key");
        Check(warnings.Count == 1 && warnings[0].Contains("no-such-message"), "a missing key is logged");

        var partial = translations.T("needs");
        Check(partial.StartsWith("Hello "), $"a missing argument still renders the English text, got \"{partial}\"");
        Check(warnings.Count == 2 && warnings[1].Contains("needs"), "a message that did not format cleanly is logged");

        Equal(translations.T("needs", ("name", "Ana")), "Hola Ana", "the active language formats when its arguments are complete");
        Check(translations.Has(Language.Spanish, "both") && !translations.Has(Language.Spanish, "only-english"),
            "Has reports a language's own bundle without the English fallback");
        return Task.CompletedTask;
    }

    /// <summary>A Spanish file that does not build must not take the app down: it speaks English.</summary>
    public static Task FallsBackToEnglishWhenSpanishDoesNotBuildAsync()
    {
        var warnings = new List<string>();
        var translations = new Translations(Language.Spanish,
            english: "both = Both\nneeds = Hello { $name }\n",
            spanish: "both = Ambos\nbroken = { $name\n= orphan value\n",
            warn: warnings.Add);
        Equal(translations.T("both"), "Both", "a Spanish bundle that failed to build is not used at all");
        Equal(translations.T("needs", ("name", "Ana")), "Hello Ana", "English still formats arguments");
        Check(warnings.Any(warning => warning.Contains("es.ftl")), "the broken Spanish bundle is logged: " + string.Join(" | ", warnings));
        Check(!translations.Has(Language.Spanish, "both"), "the broken Spanish bundle reports no messages");
        return Task.CompletedTask;
    }

    /// <summary>Should never happen (the parity test parses en.ftl), but then every lookup renders its key.</summary>
    public static Task RendersKeysWhenEnglishDoesNotBuildAsync()
    {
        var warnings = new List<string>();
        var translations = new Translations(Language.English,
            english: "broken = { $name\n= orphan value\n",
            spanish: "",
            warn: warnings.Add);
        Equal(translations.T("broken"), "broken", "a lookup renders its key");
        Equal(translations.T("home-recent-title"), "home-recent-title", "any lookup renders its key");
        Check(warnings.Any(warning => warning.Contains("en.ftl")), "the broken English bundle is logged");
        return Task.CompletedTask;
    }

    public static Task NeverThrowsWhileFormattingAsync()
    {
        var warnings = new List<string>();
        var translations = new Translations(Language.English,
            english: "needs = Hello { $name }\n", spanish: "", warn: warnings.Add);
        var text = translations.T("needs", ("name", "Ana"), ("name", "Bea"));
        Check(text.StartsWith("Hello "), $"a duplicated argument still renders the message, got \"{text}\"");
        Check(warnings.Any(warning => warning.Contains("name")), "a duplicated argument is logged");
        return Task.CompletedTask;
    }

    public static Task LogsEachMissingKeyOnceAsync()
    {
        var warnings = new List<string>();
        var translations = new Translations(Language.English, english: "a = A\n", spanish: "", warn: warnings.Add);
        for (var i = 0; i < 3; i++) Equal(translations.T("missing-key"), "missing-key", "a missing key renders as itself");
        translations.T("other-missing-key");
        Check(warnings.Count == 2, $"each missing key is logged once, got {warnings.Count}: " + string.Join(" | ", warnings));
        return Task.CompletedTask;
    }

    /// <summary>Message id → whether it has a value and its attribute names.</summary>
    private static Dictionary<string, (bool HasValue, string Attributes)> Shape(Language language)
    {
        var source = Translations.EmbeddedSource(language);
        var resource = LinguiniParser.FromFragment(source).Parse();
        Check(resource.Errors.Count == 0, $"{language} .ftl has parse errors: " + string.Join("; ", resource.Errors.Select(error => error.Message)));
        var messages = new Dictionary<string, (bool, string)>(StringComparer.Ordinal);
        foreach (var entry in resource.Entries)
        {
            if (entry is not AstMessage message) continue;
            var attributes = string.Join(",", message.Attributes.Select(attribute => attribute.Id.ToString()).Order());
            messages[message.GetId()] = (message.Value is not null, attributes);
        }
        return messages;
    }

    private static string FindShellSources()
    {
        for (var directory = new DirectoryInfo(AppContext.BaseDirectory); directory is not null; directory = directory.Parent)
        {
            var candidate = Path.Combine(directory.FullName, "apps", "windows", "Pdf.Windows");
            if (File.Exists(Path.Combine(candidate, "Pdf.Windows.csproj"))) return candidate;
        }
        throw new DirectoryNotFoundException("apps/windows/Pdf.Windows was not found above the test binaries");
    }
}
