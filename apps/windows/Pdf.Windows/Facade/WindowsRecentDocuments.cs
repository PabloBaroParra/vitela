using System.Runtime.InteropServices;

namespace Pdf.Windows.Facade;

public sealed record WindowsRecentPdf(string Path, DateTime OpenedUtc);

/// <summary>Adapter for the desktop's shared Recent shortcuts, not an app-private MRU.</summary>
public static class WindowsRecentDocuments
{
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    private static extern void SHAddToRecentDocs(uint flags, string path);

    public static string DirectoryPath => Environment.GetFolderPath(Environment.SpecialFolder.Recent);

    /// <summary>Records a successful open, and brings the document back if the reader had hidden it.</summary>
    public static void Remember(string path, HiddenRecents hidden)
    {
        if (string.IsNullOrWhiteSpace(path)) return;
        // SHARD_PATHW. The OS owns shortcut creation, privacy policy and eviction.
        SHAddToRecentDocs(3, path);
        hidden.Unhide(path);
    }

    /// <summary>Up to eight existing local PDFs, newest first, leaving out the ones the reader hid.</summary>
    public static Task<IReadOnlyList<WindowsRecentPdf>> ReadAsync(HiddenRecents hidden)
    {
        var completion = new TaskCompletionSource<IReadOnlyList<WindowsRecentPdf>>(TaskCreationOptions.RunContinuationsAsynchronously);
        var thread = new Thread(() =>
        {
            try { completion.SetResult(ReadOnSta(hidden.Load())); }
            catch { completion.SetResult(Array.Empty<WindowsRecentPdf>()); }
        }) { IsBackground = true, Name = "Vitela desktop recents" };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        return completion.Task;
    }

    private static IReadOnlyList<WindowsRecentPdf> ReadOnSta(IReadOnlyDictionary<string, DateTime> hidden)
    {
        var results = new List<WindowsRecentPdf>();
        object? shell = null, folder = null;
        try
        {
            if (!Directory.Exists(DirectoryPath)) return results;
            var type = Type.GetTypeFromProgID("Shell.Application");
            if (type is null) return results;
            shell = Activator.CreateInstance(type);
            if (shell is null) return results;
            folder = ((dynamic)shell).NameSpace(DirectoryPath);
            if (folder is null) return results;
            foreach (var shortcut in Directory.EnumerateFiles(DirectoryPath, "*.lnk")
                .OrderByDescending(File.GetLastWriteTimeUtc))
            {
                object? item = null, link = null;
                try
                {
                    item = ((dynamic)folder).ParseName(System.IO.Path.GetFileName(shortcut));
                    if (item is null) continue;
                    link = ((dynamic)item).GetLink;
                    if (link is null) continue;
                    string path = ((dynamic)link).Path;
                    if (path.StartsWith("\\\\", StringComparison.Ordinal)) continue;
                    if (!System.IO.Path.GetExtension(path).Equals(".pdf", StringComparison.OrdinalIgnoreCase) || !File.Exists(path)) continue;
                    if (results.Any(entry => entry.Path.Equals(path, StringComparison.OrdinalIgnoreCase))) continue;
                    var opened = File.GetLastWriteTimeUtc(shortcut);
                    // Before the cap, so a hidden document does not cost the list one of its eight cards.
                    if (HiddenRecents.Hides(hidden, path, opened)) continue;
                    results.Add(new WindowsRecentPdf(path, opened));
                    if (results.Count == 8) break;
                }
                catch { /* A stale or inaccessible shortcut is not a usable card. */ }
                finally { Release(link); Release(item); }
            }
        }
        finally { Release(folder); Release(shell); }
        return results;
    }

    private static void Release(object? value)
    {
        if (value is not null && Marshal.IsComObject(value)) Marshal.ReleaseComObject(value);
    }
}
