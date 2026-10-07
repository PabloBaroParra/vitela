namespace Pdf.Windows.Facade;

/// <summary>
/// The PDFs the reader removed from Vitela's Recent list. Recent itself is the
/// desktop's shared history (<see cref="WindowsRecentDocuments"/>); removing a
/// card must not edit that history, because Explorer and every other app read
/// it too. So Vitela keeps its own note of what it hides, and when.
///
/// A document stays hidden only until it is opened again: an entry whose
/// shared shortcut is newer than the moment it was hidden shows again, so
/// reopening a removed PDF from anywhere brings it back, and nothing else does.
///
/// One line per document, <c>ticks TAB path</c> — a tab cannot occur in a
/// Windows path. Written to a sibling and moved over, like the signature store.
/// Every method touches storage, so callers run them off the UI thread.
/// </summary>
public sealed class HiddenRecents(string path)
{
    /// <summary>Old entries beyond this are dropped: a hidden list must not grow forever.</summary>
    internal const int Capacity = 256;

    public static string DefaultPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "Vitela",
        "hidden-recents.txt");

    private string Partial => path + ".partial";

    /// <summary>Each hidden document's path and the UTC time it was hidden. Unreadable storage hides nothing.</summary>
    public IReadOnlyDictionary<string, DateTime> Load()
    {
        var hidden = new Dictionary<string, DateTime>(StringComparer.OrdinalIgnoreCase);
        string[] lines;
        try
        {
            if (!File.Exists(path)) return hidden;
            lines = File.ReadAllLines(path);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            return hidden;
        }
        foreach (var line in lines)
        {
            var tab = line.IndexOf('\t');
            if (tab <= 0 || tab == line.Length - 1) continue;
            if (!long.TryParse(line.AsSpan(0, tab), out var ticks) || ticks < DateTime.MinValue.Ticks || ticks > DateTime.MaxValue.Ticks) continue;
            var at = new DateTime(ticks, DateTimeKind.Utc);
            var document = line[(tab + 1)..];
            if (!hidden.TryGetValue(document, out var known) || known < at) hidden[document] = at;
        }
        return hidden;
    }

    /// <summary>Whether <paramref name="documentPath"/>, last opened at <paramref name="openedUtc"/>, is still hidden.</summary>
    public static bool Hides(IReadOnlyDictionary<string, DateTime> hidden, string documentPath, DateTime openedUtc) =>
        hidden.TryGetValue(documentPath, out var hiddenUtc) && hiddenUtc >= openedUtc;

    /// <summary>Hides <paramref name="documentPath"/> as of <paramref name="nowUtc"/>; false when the list could not be written.</summary>
    public bool Hide(string documentPath, DateTime nowUtc)
    {
        var hidden = new Dictionary<string, DateTime>(Load(), StringComparer.OrdinalIgnoreCase) { [documentPath] = nowUtc };
        return Write(hidden);
    }

    /// <summary>
    /// Forgets that <paramref name="documentPath"/> was hidden. Vitela calls it on
    /// its own successful opens, so a reopen inside the same clock tick as the
    /// removal still brings the card back.
    /// </summary>
    public void Unhide(string documentPath)
    {
        var hidden = new Dictionary<string, DateTime>(Load(), StringComparer.OrdinalIgnoreCase);
        if (hidden.Remove(documentPath)) Write(hidden);
    }

    private bool Write(Dictionary<string, DateTime> hidden)
    {
        // A document that no longer exists can never show again; keeping it would only grow the file.
        var lines = hidden
            .Where(entry => File.Exists(entry.Key))
            .OrderByDescending(entry => entry.Value)
            .Take(Capacity)
            .Select(entry => $"{entry.Value.Ticks}\t{entry.Key}");
        try
        {
            if (Path.GetDirectoryName(path) is { Length: > 0 } directory) Directory.CreateDirectory(directory);
            File.WriteAllLines(Partial, lines);
            File.Move(Partial, path, overwrite: true);
            return true;
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            try { File.Delete(Partial); } catch (Exception) { /* a leftover sibling is overwritten by the next write */ }
            return false;
        }
    }
}
