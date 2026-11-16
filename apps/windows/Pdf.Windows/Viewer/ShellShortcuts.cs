namespace Pdf.Windows.Viewer;

/// <summary>One shell keyboard shortcut: Ctrl plus a letter key.</summary>
/// <param name="Name">What the shortcut does, as Home's shortcut card names it.</param>
/// <param name="Key">The <c>VirtualKey</c> letter bound in <c>MainWindow.xaml</c>.</param>
internal sealed record ShellShortcut(string Name, char Key)
{
    /// <summary>The text the user reads, e.g. "Ctrl+O".</summary>
    public string Label => $"Ctrl+{Key}";
}

/// <summary>
/// The window shortcuts, written down once (B27 decision 7). The command
/// palette's rows and Home's shortcut card both print these, and the facade
/// suite checks that every one is bound as a <c>KeyboardAccelerator</c> in
/// <c>MainWindow.xaml</c>, so the key the user reads is the key the window
/// listens for.
/// </summary>
internal static class ShellShortcuts
{
    public static readonly ShellShortcut Open = new("Open", 'O');
    public static readonly ShellShortcut New = new("New", 'N');
    public static readonly ShellShortcut Save = new("Save", 'S');
    public static readonly ShellShortcut Find = new("Find", 'F');
    public static readonly ShellShortcut Print = new("Print", 'P');
    public static readonly ShellShortcut Undo = new("Undo", 'Z');
    public static readonly ShellShortcut Redo = new("Redo", 'Y');
    /// <summary>Opens the command palette (B27), which reaches every other command.</summary>
    public static readonly ShellShortcut CommandPalette = new("Command palette", 'K');

    public static readonly IReadOnlyList<ShellShortcut> All = [Open, New, Save, Find, Print, Undo, Redo, CommandPalette];
}
