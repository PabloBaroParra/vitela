namespace Pdf.Windows.Viewer;

/// <summary>One section of the app, in mode-bar order (GTK <c>mode_bar.rs</c> <c>Mode</c>).</summary>
public enum ShellMode { Home, Edit, Annotate, Organize, Sign, Protect }

/// <summary>Which of the window's three views is on screen.</summary>
public enum ShellScreen { Home, Editor, Organize }

/// <summary>
/// The mode bar's pure half. The bar never stores which mode is current:
/// <see cref="Current"/> derives it from the view on screen and the tools
/// panel's page, so a screen changed from anywhere else — the palette, a Home
/// tool, a shortcut, an open landing, Organize refusing — still shows as the
/// checked mode without that path knowing the bar exists.
/// </summary>
public static class ShellModes
{
    /// <summary>Tools-panel page keys. Comments is a section of Annotate, not a page.</summary>
    public const string EditPage = "Edit", AnnotatePage = "Annotate", SignPage = "Sign";

    public static IReadOnlyList<ShellMode> All { get; } =
        [ShellMode.Home, ShellMode.Edit, ShellMode.Annotate, ShellMode.Organize, ShellMode.Sign, ShellMode.Protect];

    /// <summary>The tab's label, GTK's <c>mode-*</c> strings.</summary>
    public static string Label(ShellMode mode) => mode switch
    {
        ShellMode.Home => "Home",
        ShellMode.Edit => "Edit",
        ShellMode.Annotate => "Annotate",
        ShellMode.Organize => "Organize",
        ShellMode.Sign => "Sign",
        ShellMode.Protect => "Protect",
        _ => throw new ArgumentOutOfRangeException(nameof(mode), mode, null),
    };

    public static ShellIcon Icon(ShellMode mode) => mode switch
    {
        ShellMode.Home => ShellIcon.Home,
        ShellMode.Edit => ShellIcon.Edit,
        ShellMode.Annotate => ShellIcon.Annotate,
        ShellMode.Organize => ShellIcon.Organize,
        ShellMode.Sign => ShellIcon.Sign,
        ShellMode.Protect => ShellIcon.Protect,
        _ => throw new ArgumentOutOfRangeException(nameof(mode), mode, null),
    };

    /// <summary>
    /// Whether the window can be in this mode. Protect cannot: it opens a
    /// dialog over whatever screen is up, so it is a plain button and never
    /// the checked tab.
    /// </summary>
    public static bool IsPlace(ShellMode mode) => mode != ShellMode.Protect;

    /// <summary>
    /// The view on screen from the three views' visibility. Home is shown over
    /// the open document, and Organize replaces the editor, so mid-switch the
    /// one being shown wins over the one still collapsing. Null when none is
    /// visible, which only happens between collapsing one view and showing the next.
    /// </summary>
    public static ShellScreen? ScreenOf(bool homeVisible, bool editorVisible, bool organizeVisible) =>
        homeVisible ? ShellScreen.Home
        : organizeVisible ? ShellScreen.Organize
        : editorVisible ? ShellScreen.Editor
        : null;

    /// <summary>
    /// The mode the window is in. The editor reads its mode from the tools
    /// panel's page; Annotate is that panel's default, so any page that is not
    /// Edit or Fill &amp; Sign reads as Annotate (GTK <c>Mode::of_screen</c>).
    /// </summary>
    public static ShellMode? Current(ShellScreen? screen, string? toolPage) => screen switch
    {
        ShellScreen.Home => ShellMode.Home,
        ShellScreen.Organize => ShellMode.Organize,
        ShellScreen.Editor => toolPage switch
        {
            EditPage => ShellMode.Edit,
            SignPage => ShellMode.Sign,
            _ => ShellMode.Annotate,
        },
        _ => null,
    };
}
