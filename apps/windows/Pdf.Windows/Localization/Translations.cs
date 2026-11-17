using System.Globalization;
using Linguini.Bundle;
using Linguini.Bundle.Builder;
using Linguini.Shared.Types.Bundle;

namespace Pdf.Windows.Localization;

/// <summary>
/// The translations themselves: one Fluent bundle per language and formatting
/// a message out of them, with the GTK shell's fallbacks (app/i18n/mod.rs) —
/// a message missing from the active language falls back to English, and one
/// missing from English too renders as its own key and is logged.
/// </summary>
/// <remarks>
/// The .ftl sources live in apps/windows/locales/ and are embedded in the
/// assembly (Pdf.Windows.csproj), so there is nothing to install or find at
/// run time and a packaged build cannot ship without them.
/// </remarks>
public sealed class Translations
{
    private readonly Language _active;
    private readonly FluentBundle _english;
    private readonly FluentBundle _spanish;
    private readonly Action<string> _warn;
    // FluentBundle is not documented as thread-safe; formatting is cheap, so serialize it.
    private readonly object _gate = new();

    public Translations(Language active, string english, string spanish, Action<string>? warn = null)
    {
        _active = active;
        _english = Build(Language.English, english);
        _spanish = Build(Language.Spanish, spanish);
        _warn = warn ?? (message => System.Diagnostics.Trace.TraceWarning(message));
    }

    /// <summary>The language this instance formats in.</summary>
    public Language Active => _active;

    /// <summary>The bundles built from the embedded en.ftl/es.ftl, speaking <paramref name="active"/>.</summary>
    public static Translations Embedded(Language active, Action<string>? warn = null) =>
        new(active, EmbeddedSource(Language.English), EmbeddedSource(Language.Spanish), warn);

    /// <summary>The embedded .ftl text for <paramref name="language"/>.</summary>
    public static string EmbeddedSource(Language language)
    {
        var name = $"Pdf.Windows.Locales.{language.Tag()}.ftl";
        using var stream = typeof(Translations).Assembly.GetManifestResourceStream(name)
            ?? throw new InvalidOperationException($"The embedded translation {name} is missing from the build.");
        using var reader = new StreamReader(stream);
        return reader.ReadToEnd();
    }

    /// <summary>Message <paramref name="key"/> in the active language, with Fluent arguments.</summary>
    public string T(string key, params (string Name, object Value)[] args)
    {
        var fluentArgs = args.Length == 0 ? null : args.ToDictionary(arg => arg.Name, arg => ToFluent(arg.Value));
        lock (_gate)
        {
            if (Render(Bundle(_active), key, fluentArgs) is { Complete: true } active) return active.Text;
            var english = Render(_english, key, fluentArgs);
            if (english is null)
            {
                _warn($"i18n: no English message \"{key}\"");
                return key;
            }
            if (!english.Value.Complete) _warn($"i18n: message \"{key}\" did not format cleanly: \"{english.Value.Text}\"");
            return english.Value.Text;
        }
    }

    /// <summary>Whether <paramref name="language"/>'s own bundle has <paramref name="key"/>, without the English fallback.</summary>
    public bool Has(Language language, string key)
    {
        lock (_gate) return Bundle(language).HasMessage(key);
    }

    private FluentBundle Bundle(Language language) => language == Language.Spanish ? _spanish : _english;

    private static FluentBundle Build(Language language, string source) =>
        // Without SetUseIsolating(false) Fluent wraps every placeable in
        // U+2068/U+2069, which some fonts draw as visible boxes. Unchecked: a
        // parse error would throw here; the parity test keeps both files clean.
        LinguiniBuilder.Builder()
            .CultureInfo(new CultureInfo(language.Tag()))
            .AddResource(source)
            .SetUseIsolating(false)
            .UncheckedBuild();

    /// <summary>The formatted message and whether Fluent reported no errors; null when there is no such message.</summary>
    private static (string Text, bool Complete)? Render(FluentBundle bundle, string key, IDictionary<string, IFluentType>? args)
    {
        if (!bundle.HasMessage(key)) return null;
        var complete = bundle.TryGetAttrMessage(key, args, out var errors, out var text);
        return (text ?? "", complete && (errors is null || errors.Count == 0));
    }

    private static IFluentType ToFluent(object value) => value switch
    {
        string text => (FluentString)text,
        int number => (FluentNumber)number,
        long number => (FluentNumber)number,
        uint number => (FluentNumber)number,
        ulong number => (FluentNumber)(double)number,
        double number => (FluentNumber)number,
        float number => (FluentNumber)number,
        decimal number => (FluentNumber)(double)number,
        _ => (FluentString)(Convert.ToString(value, CultureInfo.CurrentCulture) ?? ""),
    };
}
