using Pdf.Windows.Viewer;

/// <summary>
/// The mode bar's contract: six modes in the GTK bar's order, Protect the one
/// that is never a place, and the checked mode derived from the screen alone.
/// Ported from the GTK shell's <c>mode_bar/tests.rs</c>.
/// </summary>
internal static class ShellModeTests
{
    public static Task RunAsync()
    {
        TheBarListsTheSixModesInTheLinuxOrder();
        EveryModeHasItsOwnLabelAndIcon();
        ProtectIsTheOnlyModeThatIsNotAPlace();
        HomeAndOrganizeAreReadFromTheScreenAlone();
        TheEditorReadsItsModeFromTheToolsPage();
        AnUnknownToolsPageReadsAsAnnotate();
        NoScreenMeansNoCheckedMode();
        HomeOutranksTheViewsBehindIt();
        OrganizeOutranksTheEditorItReplaced();
        NoVisibleViewIsNoScreen();
        NoScreenEverReadsAsProtect();
        return Task.CompletedTask;
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static void TheBarListsTheSixModesInTheLinuxOrder() =>
        Check(ShellModes.All.SequenceEqual([ShellMode.Home, ShellMode.Edit, ShellMode.Annotate, ShellMode.Organize, ShellMode.Sign, ShellMode.Protect]),
            "The mode bar lists Home, Edit, Annotate, Organize, Sign, Protect, the GTK bar's order");

    private static void EveryModeHasItsOwnLabelAndIcon()
    {
        Check(ShellModes.All.Select(ShellModes.Label).SequenceEqual(["Home", "Edit", "Annotate", "Organize", "Sign", "Protect"]),
            "Mode labels are the GTK en.ftl mode-* strings");
        Check(ShellModes.All.Select(ShellModes.Icon).SequenceEqual(
                [ShellIcon.Home, ShellIcon.Edit, ShellIcon.Annotate, ShellIcon.Organize, ShellIcon.Sign, ShellIcon.Protect]),
            "Each mode draws the shared icon of the same name");
    }

    private static void ProtectIsTheOnlyModeThatIsNotAPlace()
    {
        // Protect opens a dialog over whatever screen is up, so it is a plain button.
        Check(!ShellModes.IsPlace(ShellMode.Protect), "Protect is an action, never the checked tab");
        Check(ShellModes.All.Where(mode => mode != ShellMode.Protect).All(ShellModes.IsPlace), "Every other mode is a place");
    }

    private static void HomeAndOrganizeAreReadFromTheScreenAlone()
    {
        foreach (var page in new string?[] { null, ShellModes.EditPage, ShellModes.AnnotatePage, ShellModes.SignPage })
        {
            Check(ShellModes.Current(ShellScreen.Home, page) == ShellMode.Home, $"Home is Home whatever the tools page ({page})");
            Check(ShellModes.Current(ShellScreen.Organize, page) == ShellMode.Organize, $"Organize is Organize whatever the tools page ({page})");
        }
    }

    private static void TheEditorReadsItsModeFromTheToolsPage()
    {
        Check(ShellModes.Current(ShellScreen.Editor, ShellModes.EditPage) == ShellMode.Edit, "The Edit page reads as Edit");
        Check(ShellModes.Current(ShellScreen.Editor, ShellModes.AnnotatePage) == ShellMode.Annotate, "The Annotate page reads as Annotate");
        Check(ShellModes.Current(ShellScreen.Editor, ShellModes.SignPage) == ShellMode.Sign, "The Fill & Sign page reads as Sign");
    }

    private static void AnUnknownToolsPageReadsAsAnnotate()
    {
        // Annotate is the tools panel's default page, as in GTK's of_screen.
        Check(ShellModes.Current(ShellScreen.Editor, null) == ShellMode.Annotate, "An editor with no page yet reads as Annotate");
        Check(ShellModes.Current(ShellScreen.Editor, "Comments") == ShellMode.Annotate, "Comments is part of Annotate, not a mode");
    }

    private static void NoScreenMeansNoCheckedMode() =>
        Check(ShellModes.Current(null, ShellModes.EditPage) is null, "A window with no view on screen checks no mode");

    private static void HomeOutranksTheViewsBehindIt()
    {
        Check(ShellModes.ScreenOf(homeVisible: true, editorVisible: false, organizeVisible: false) == ShellScreen.Home, "Home alone is Home");
        Check(ShellModes.ScreenOf(homeVisible: true, editorVisible: true, organizeVisible: false) == ShellScreen.Home,
            "Mid-switch, Home on screen wins over the editor still collapsing");
        Check(ShellModes.ScreenOf(homeVisible: true, editorVisible: false, organizeVisible: true) == ShellScreen.Home,
            "Mid-switch, Home on screen wins over Organize still collapsing");
    }

    private static void OrganizeOutranksTheEditorItReplaced()
    {
        Check(ShellModes.ScreenOf(homeVisible: false, editorVisible: false, organizeVisible: true) == ShellScreen.Organize, "Organize alone is Organize");
        Check(ShellModes.ScreenOf(homeVisible: false, editorVisible: true, organizeVisible: true) == ShellScreen.Organize,
            "Mid-switch, Organize wins over the editor it replaces");
        Check(ShellModes.ScreenOf(homeVisible: false, editorVisible: true, organizeVisible: false) == ShellScreen.Editor, "The editor alone is the editor");
    }

    private static void NoVisibleViewIsNoScreen() =>
        Check(ShellModes.ScreenOf(homeVisible: false, editorVisible: false, organizeVisible: false) is null,
            "Between collapsing one view and showing the next there is no screen");

    private static void NoScreenEverReadsAsProtect()
    {
        foreach (var screen in new ShellScreen?[] { null, ShellScreen.Home, ShellScreen.Editor, ShellScreen.Organize })
            foreach (var page in new string?[] { null, "Protect", ShellModes.EditPage, ShellModes.AnnotatePage, ShellModes.SignPage })
                Check(ShellModes.Current(screen, page) != ShellMode.Protect, $"Protect is never checked ({screen}, {page})");
    }
}
