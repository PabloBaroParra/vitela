namespace Pdf.Windows.Viewer;

/// <summary>Where one file of an open lands.</summary>
public enum TabOpenRoute
{
    /// <summary>A new tab, shown at once; the tab that was on screen is parked.</summary>
    OpenInFront,
    /// <summary>A new tab behind the one on screen, which stays where it is.</summary>
    OpenInBackground,
    /// <summary>A background tab already holds the file: bring it forward.</summary>
    SwitchTo,
    /// <summary>The tab on screen holds the file: read it again, asking first if it has unsaved changes.</summary>
    Reload,
    /// <summary>Nothing to do: a background open of a file some tab already holds.</summary>
    Skip,
}

/// <summary>
/// The placement rule for opens, a port of the GTK shell's <c>tabs/open.rs</c>:
/// an open lands in front only when it is the first file of a user action and
/// the active tab can give way; every other open lands in a background tab.
/// A file already open in a tab is never opened twice.
/// </summary>
public static class TabOpenRouting
{
    /// <summary>
    /// Whether file <paramref name="index"/> of a user action may land in front:
    /// the first one, or any later one while no tab is active — the files
    /// before it failed with nothing on screen, and a batch that only landed in
    /// the background would leave its tabs unreachable from Home.
    /// </summary>
    public static bool IsFront(int index, bool anyTabActive) => index == 0 || !anyTabActive;

    /// <param name="front">Whether this is the first file of the user action.</param>
    /// <param name="frontAllowed">Whether the active tab can give way now (its inline edit settled).</param>
    /// <param name="holder">The tab already holding the file, if any.</param>
    /// <param name="active">The tab on screen, if any.</param>
    public static TabOpenRoute Route(bool front, bool frontAllowed, TabId? holder, TabId? active)
    {
        var inFront = front && frontAllowed;
        if (holder is { } held)
        {
            if (!inFront) return TabOpenRoute.Skip;
            return held == active ? TabOpenRoute.Reload : TabOpenRoute.SwitchTo;
        }
        return inFront ? TabOpenRoute.OpenInFront : TabOpenRoute.OpenInBackground;
    }
}
