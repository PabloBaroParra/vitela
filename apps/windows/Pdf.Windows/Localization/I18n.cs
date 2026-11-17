namespace Pdf.Windows.Localization;

/// <summary>
/// User-facing text, in the user's language — the Windows port of the GTK
/// shell's app/i18n module, sharing its message keys and Spanish text.
/// </summary>
/// <remarks>
/// <para>Code: <c>I18n.T("home-recent-title")</c>, or with arguments
/// <c>I18n.T("home-recent-page-count", ("count", pages))</c>. XAML:
/// <c>Text="{i18n:T Key=home-welcome-title}"</c> (TExtension).</para>
/// <para>The language is chosen once per process (<see cref="LanguageNegotiation"/>).
/// The facade tests always start in English, whatever the machine's language;
/// a test that needs Spanish takes a <see cref="Speak"/> guard.</para>
/// <para>Deliberate exceptions, which stay as they are in every language, as
/// on Linux: technical details from the core and the OS (they arrive inside a
/// localized sentence as an argument), the product name, keyboard shortcuts and
/// number formats such as <c>3 / 12</c> or <c>100%</c>.</para>
/// </remarks>
public static class I18n
{
    private static readonly Lazy<Translations>[] Bundles =
    [
        new(() => Translations.Embedded(Language.English)),
        new(() => Translations.Embedded(Language.Spanish)),
    ];

    private static Language _active = InitialLanguage();

#if VITELA_FACADE_TESTS
    // Deterministic tests: English, regardless of the developer's display language.
    private static Language InitialLanguage() => Language.English;
#else
    private static Language InitialLanguage() => LanguageNegotiation.FromEnvironment();
#endif

    /// <summary>The language the shell is speaking.</summary>
    public static Language Active => _active;

    /// <summary>Message <paramref name="key"/> in the active language, with Fluent arguments.</summary>
    public static string T(string key, params (string Name, object Value)[] args) =>
        Bundles[(int)_active].Value.T(key, args);

    /// <summary>
    /// Switches to <paramref name="language"/> until the guard is disposed. A
    /// guard rather than a setter so a test cannot leave a language switched for
    /// every later case. The shell itself never switches at run time.
    /// </summary>
    public static IDisposable Speak(Language language)
    {
        var previous = _active;
        _active = language;
        return new Restore(previous);
    }

    private sealed class Restore(Language previous) : IDisposable
    {
        private bool _disposed;

        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            _active = previous;
        }
    }
}
