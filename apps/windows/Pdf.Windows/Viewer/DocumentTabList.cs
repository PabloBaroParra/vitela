namespace Pdf.Windows.Viewer;

/// <summary>
/// One tab's identity for the lifetime of the window. Never reused, so a stale
/// id (a click on a tab that has since closed) names nothing rather than some
/// newer tab.
/// </summary>
public readonly record struct TabId(long Value);

/// <summary>What closing a tab leaves active.</summary>
public enum AfterCloseKind
{
    /// <summary>The closed tab was not the active one: the active tab stays.</summary>
    Unchanged,
    /// <summary>The closed tab was active; <see cref="AfterClose.Next"/> should be activated next.</summary>
    Activate,
    /// <summary>The closed tab was the last one.</summary>
    Empty,
}

/// <summary>The answer to <see cref="DocumentTabList.Close"/>.</summary>
public readonly record struct AfterClose(AfterCloseKind Kind, TabId? Next = null);

/// <summary>
/// The window's document tabs in strip order, which one is active, and the
/// file each was opened from or last saved to. Holds no documents: the window
/// keeps each tab's facade beside it. A port of the GTK shell's
/// <c>tabs/list.rs</c>.
/// </summary>
public sealed class DocumentTabList
{
    private readonly List<Entry> _tabs = [];
    private long _nextId = 1;

    private sealed class Entry(TabId id, string? path)
    {
        public TabId Id { get; } = id;
        public string? Path { get; set; } = path;
    }

    /// <summary>The tab whose document is on screen; <c>null</c> while none is.</summary>
    public TabId? Active { get; private set; }

    public int Count => _tabs.Count;

    public bool IsEmpty => _tabs.Count == 0;

    /// <summary>Every tab, in strip order.</summary>
    public IReadOnlyList<TabId> Ids => _tabs.Select(tab => tab.Id).ToList();

    /// <summary>
    /// Appends a tab at the end of the strip. It is not activated: the caller
    /// decides when the document behind it is on screen.
    /// </summary>
    /// <param name="path">The file it came from; normalised here, <c>null</c> for a sample, blank or restored document.</param>
    public TabId Add(string? path)
    {
        var id = new TabId(_nextId++);
        _tabs.Add(new Entry(id, TabPaths.Normalize(path)));
        return id;
    }

    /// <summary>Makes <paramref name="id"/> the active tab; <c>false</c>, and no change, for an id not in the list.</summary>
    public bool Activate(TabId id)
    {
        if (!Contains(id)) return false;
        Active = id;
        return true;
    }

    /// <summary>No tab is active: the one that was has been parked, and the next one is not installed yet.</summary>
    public void Deactivate() => Active = null;

    public bool Contains(TabId id) => _tabs.Any(tab => tab.Id == id);

    /// <summary>
    /// Removes <paramref name="id"/>, answering what should become active.
    /// <c>null</c> when <paramref name="id"/> is not in the list. Closing the
    /// active tab leaves none active: the caller activates the next one once
    /// its document is installed.
    /// </summary>
    public AfterClose? Close(TabId id)
    {
        var index = _tabs.FindIndex(tab => tab.Id == id);
        if (index < 0) return null;
        var after = NextActiveAfterClose(Ids, id, Active);
        _tabs.RemoveAt(index);
        if (Active == id) Active = null;
        return after;
    }

    /// <summary>
    /// Which tab takes over when <paramref name="closing"/> is closed out of
    /// <paramref name="order"/> while <paramref name="active"/> is active. The
    /// right-hand neighbour first, as browsers and editors do — the strip does
    /// not shift under the pointer — and the left-hand one only when the closed
    /// tab was last.
    /// </summary>
    public static AfterClose NextActiveAfterClose(IReadOnlyList<TabId> order, TabId closing, TabId? active)
    {
        if (active != closing) return new AfterClose(AfterCloseKind.Unchanged);
        var index = order.ToList().IndexOf(closing);
        if (index < 0) return new AfterClose(AfterCloseKind.Unchanged);
        if (index + 1 < order.Count) return new AfterClose(AfterCloseKind.Activate, order[index + 1]);
        if (index > 0) return new AfterClose(AfterCloseKind.Activate, order[index - 1]);
        return new AfterClose(AfterCloseKind.Empty);
    }

    /// <summary>The file tab <paramref name="id"/> holds, normalised; <c>null</c> for none or an unknown tab.</summary>
    public string? PathOf(TabId id) => _tabs.FirstOrDefault(tab => tab.Id == id)?.Path;

    /// <summary>Records where <paramref name="id"/>'s document now lives — a Save As moves it.</summary>
    public void SetPath(TabId id, string? path)
    {
        if (_tabs.FirstOrDefault(tab => tab.Id == id) is { } entry) entry.Path = TabPaths.Normalize(path);
    }

    /// <summary>
    /// The tab holding <paramref name="path"/>. When several do, the active one
    /// wins, then the first in strip order.
    /// </summary>
    public TabId? FindByPath(string? path)
    {
        var holders = Holders(path);
        if (holders.Count == 0) return null;
        return Active is { } active && holders.Contains(active) ? active : holders[0];
    }

    /// <summary>Every tab holding <paramref name="path"/>, in strip order.</summary>
    public IReadOnlyList<TabId> Holders(string? path)
    {
        if (TabPaths.Normalize(path) is not { } wanted) return [];
        return _tabs.Where(tab => tab.Path is { } held && TabPaths.Same(held, wanted)).Select(tab => tab.Id).ToList();
    }

    /// <summary>
    /// Whether a tab other than <paramref name="self"/> holds <paramref name="path"/>
    /// — what Save As refuses, so two tabs never claim one file.
    /// </summary>
    public bool HeldByAnotherTab(string? path, TabId? self) => Holders(path).Any(id => id != self);

    /// <summary>
    /// The tab <paramref name="step"/> places from the active one, wrapping
    /// around the strip (Ctrl+Tab is +1, Ctrl+Shift+Tab −1). <c>null</c> when
    /// there is no other tab to go to.
    /// </summary>
    public TabId? Cycle(int step)
    {
        if (_tabs.Count < 2 || Active is not { } active) return null;
        var index = _tabs.FindIndex(tab => tab.Id == active);
        var next = ((index + step) % _tabs.Count + _tabs.Count) % _tabs.Count;
        return next == index ? null : _tabs[next].Id;
    }
}

/// <summary>How a tab's file is compared: a full path, case-insensitive as Windows file names are.</summary>
public static class TabPaths
{
    /// <summary>
    /// The full path of <paramref name="path"/>, or <c>null</c> when it names no
    /// file a tab can hold — none at all, or one that is not fully qualified
    /// (a storage item without a file-system path reports an empty one).
    /// </summary>
    public static string? Normalize(string? path)
    {
        if (string.IsNullOrWhiteSpace(path) || !System.IO.Path.IsPathFullyQualified(path)) return null;
        try
        {
            return System.IO.Path.GetFullPath(path);
        }
        catch (Exception error) when (error is ArgumentException or NotSupportedException or PathTooLongException)
        {
            return null;
        }
    }

    public static bool Same(string left, string right) => string.Equals(left, right, StringComparison.OrdinalIgnoreCase);
}
