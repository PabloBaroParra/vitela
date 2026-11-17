using System.Globalization;
using Linguini.Bundle;
using Linguini.Bundle.Builder;
using Linguini.Shared.Types.Bundle;

namespace Pdf.Windows.Localization;

/// <summary>
/// The translations themselves: one Fluent bundle per language and formatting
/// a message out of them, with the GTK shell's fallbacks (app/i18n/mod.rs) —
/// a message missing from the active language falls back to English, and one
/// missing from English too renders as its own key and is logged. Nothing
/// here throws: a translation that does not build, or a message that fails to
/// format, is logged and degrades to English or to the key.
/// </summary>
/// <remarks>
/// The .ftl sources live in apps/windows/locales/ and are embedded in the
/// assembly (Pdf.Windows.csproj), so there is nothing to install or find at
/// run time and a packaged build cannot ship without them.
/// </remarks>
public sealed class Translations
{
    private readonly Language _active;
    // Null when the source did not build: an English that failed renders every
    // message as its key; a Spanish that failed leaves the shell in English.
    private readonly FluentBundle? _english;
    private readonly FluentBundle? _spanish;
    private readonly Action<string> _warn;
    // Each distinct warning is written once, so a missing key looked up on every
    // repaint does not flood the log.
    private readonly HashSet<string> _warned = new(StringComparer.Ordinal);
    // FluentBundle is not documented as thread-safe; formatting is cheap, so serialize it.
    private readonly object _gate = new();

    /// <summary>
    /// Builds English and, when it is the active language, Spanish. Never
    /// throws: a source that does not build is logged and left out.
    /// </summary>
    public Translations(Language active, string english, string spanish, Action<string>? warn = null)
    {
        _active = active;
        _warn = warn ?? (message => System.Diagnostics.Trace.TraceWarning(message));
        _english = Build(Language.English, english);
        _spanish = active == Language.Spanish ? Build(Language.Spanish, spanish) : null;
    }

    /// <summary>The language this instance formats in.</summary>
    public Language Active => _active;

    /// <summary>The bundles built from the embedded en.ftl/es.ftl, speaking <paramref name="active"/>.</summary>
    public static Translations Embedded(Language active, Action<string>? warn = null)
    {
        warn ??= message => System.Diagnostics.Trace.TraceWarning(message);
        return new(active,
            EmbeddedSourceOrEmpty(Language.English, warn),
            active == Language.Spanish ? EmbeddedSourceOrEmpty(Language.Spanish, warn) : "",
            warn);
    }

    /// <summary>The embedded .ftl text for <paramref name="language"/>.</summary>
    public static string EmbeddedSource(Language language)
    {
        var name = $"Pdf.Windows.Locales.{language.Tag()}.ftl";
        using var stream = typeof(Translations).Assembly.GetManifestResourceStream(name)
            ?? throw new InvalidOperationException($"The embedded translation {name} is missing from the build.");
        using var reader = new StreamReader(stream);
        return reader.ReadToEnd();
    }

    /// <summary>Message <paramref name="key"/> in the active language, with Fluent arguments. Never throws.</summary>
    public string T(string key, params (string Name, object Value)[] args)
    {
        lock (_gate)
        {
            try
            {
                var fluentArgs = Arguments(key, args);
                if (Render(Bundle(_active), key, fluentArgs) is { Complete: true } active) return active.Text;
                var english = Render(_english, key, fluentArgs);
                if (english is null)
                {
                    Warn($"i18n: no English message \"{key}\"");
                    return key;
                }
                if (!english.Value.Complete) Warn($"i18n: message \"{key}\" did not format cleanly: \"{english.Value.Text}\"");
                return english.Value.Text;
            }
            catch (Exception error)
            {
                Warn($"i18n: message \"{key}\" failed to format: {error.GetType().Name}: {error.Message}");
                return key;
            }
        }
    }

    /// <summary>Whether <paramref name="language"/>'s own bundle has <paramref name="key"/>, without the English fallback.</summary>
    public bool Has(Language language, string key)
    {
        lock (_gate) return Bundle(language)?.HasMessage(key) ?? false;
    }

    private FluentBundle? Bundle(Language language) => language == Language.Spanish ? _spanish : _english;

    private void Warn(string message)
    {
        if (_warned.Add(message)) _warn(message);
    }

    /// <summary>The bundle for <paramref name="source"/>, or null when it does not build (UncheckedBuild throws on a parse error).</summary>
    private FluentBundle? Build(Language language, string source)
    {
        try
        {
            // Without SetUseIsolating(false) Fluent wraps every placeable in
            // U+2068/U+2069, which some fonts draw as visible boxes.
            return LinguiniBuilder.Builder()
                .CultureInfo(new CultureInfo(language.Tag()))
                .AddResource(source)
                .SetUseIsolating(false)
                .UncheckedBuild();
        }
        catch (Exception error)
        {
            Warn($"i18n: {language.Tag()}.ftl failed to build, so it is not used: {error.GetType().Name}: {error.Message}");
            return null;
        }
    }

    private static string EmbeddedSourceOrEmpty(Language language, Action<string> warn)
    {
        try
        {
            return EmbeddedSource(language);
        }
        catch (Exception error)
        {
            warn($"i18n: {error.Message}");
            return "";
        }
    }

    /// <summary>The Fluent arguments; a duplicated name keeps its last value and is logged instead of throwing.</summary>
    private Dictionary<string, IFluentType>? Arguments(string key, (string Name, object Value)[] args)
    {
        if (args.Length == 0) return null;
        var fluentArgs = new Dictionary<string, IFluentType>(StringComparer.Ordinal);
        foreach (var (name, value) in args)
        {
            if (fluentArgs.ContainsKey(name)) Warn($"i18n: message \"{key}\" was given argument \"{name}\" twice");
            fluentArgs[name] = ToFluent(value);
        }
        return fluentArgs;
    }

    /// <summary>The formatted message and whether Fluent reported no errors; null when there is no such message.</summary>
    private static (string Text, bool Complete)? Render(FluentBundle? bundle, string key, IDictionary<string, IFluentType>? args)
    {
        if (bundle is null || !bundle.HasMessage(key)) return null;
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
