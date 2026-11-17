using System.Globalization;

namespace Pdf.Windows.Localization;

/// <summary>A language the shell has a translation for.</summary>
public enum Language
{
    English,
    Spanish,
}

/// <summary>
/// Which language the shell speaks. The same rule as the GTK shell's
/// app/i18n/locale.rs: any Spanish locale (es, es-ES, es-419, …) is Spanish,
/// everything else is English. Linux reads the POSIX locale variables; Windows
/// reads the user's display language (CultureInfo.CurrentUICulture), with
/// <see cref="OverrideVariable"/> consulted first, the way LANGUAGE is on Linux.
/// </summary>
/// <remarks>
/// <see cref="Resolve"/> is pure — it is handed both inputs — so the rules can be
/// pinned by a test that does not depend on the developer's own language.
/// </remarks>
public static class LanguageNegotiation
{
    /// <summary>
    /// Forces a language (<c>es</c>, <c>en</c>) regardless of the Windows display
    /// language. Like Linux's LANGUAGE it may be a colon-separated list
    /// (<c>fr:es</c>); the first supported entry wins.
    /// </summary>
    public const string OverrideVariable = "VITELA_LANGUAGE";

    /// <summary>The first supported entry of the override, else the UI culture's language, else English.</summary>
    public static Language Resolve(string? overrideValue, string? uiCultureName) =>
        FirstSupported(overrideValue) ?? Supported(uiCultureName) ?? Language.English;

    /// <summary><see cref="Resolve"/> applied to this process.</summary>
    public static Language FromEnvironment() =>
        Resolve(Environment.GetEnvironmentVariable(OverrideVariable), CultureInfo.CurrentUICulture.Name);

    /// <summary>The BCP 47 tag Fluent selects plural rules by.</summary>
    public static string Tag(this Language language) => language switch
    {
        Language.Spanish => "es",
        _ => "en",
    };

    private static Language? FirstSupported(string? list) =>
        list?.Split(':').Select(Supported).FirstOrDefault(language => language is not null);

    /// <summary>The language a culture name asks for, if it is one we have: es, es-ES, es_AR and es-419 are all Spanish.</summary>
    private static Language? Supported(string? value)
    {
        var primary = value?.Trim().Split('-', '_', '.', '@')[0];
        return primary?.ToLowerInvariant() switch
        {
            "es" => Language.Spanish,
            "en" => Language.English,
            _ => null,
        };
    }
}
