using Pdf.Windows.Facade;

/// <summary>Recent cards the reader removed: hidden until reopened, without editing the desktop's shared history.</summary>
internal static class HiddenRecentsTests
{
    public static Task RunAsync()
    {
        var root = Path.Combine(Path.GetTempPath(), "vitela-hidden-recents-" + Guid.NewGuid().ToString("N"));
        try
        {
            AMissingListHidesNothing(root);
            AHiddenDocumentStaysHiddenUntilItIsOpenedAgain(root);
            HidingSurvivesANewStoreInstance(root);
            PathsCompareWithoutCase(root);
            UnhideBringsItBack(root);
            ADocumentThatNoLongerExistsIsDropped(root);
            TheListIsCapped(root);
            MalformedLinesAreSkipped(root);
            AFailedWriteReportsFailureAndKeepsTheOldList(root);
            TheDefaultPathIsLocalNotRoaming();
        }
        finally
        {
            if (Directory.Exists(root)) Directory.Delete(root, recursive: true);
        }
        return Task.CompletedTask;
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static string FreshStore(string root) => Path.Combine(root, Guid.NewGuid().ToString("N"), "hidden-recents.txt");

    private static string Document(string root, string name = "report.pdf")
    {
        var path = Path.Combine(root, "docs", Guid.NewGuid().ToString("N"), name);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllBytes(path, [37, 80, 68, 70]);
        return path;
    }

    private static readonly DateTime Noon = new(2026, 10, 7, 12, 0, 0, DateTimeKind.Utc);

    private static void AMissingListHidesNothing(string root)
    {
        var hidden = new HiddenRecents(FreshStore(root)).Load();
        Check(hidden.Count == 0, "a missing list hides nothing");
    }

    private static void AHiddenDocumentStaysHiddenUntilItIsOpenedAgain(string root)
    {
        var store = new HiddenRecents(FreshStore(root));
        var document = Document(root);

        Check(store.Hide(document, Noon), "hide succeeds");
        var hidden = store.Load();
        Check(HiddenRecents.Hides(hidden, document, Noon.AddHours(-1)), "a card opened before the removal stays hidden");
        Check(HiddenRecents.Hides(hidden, document, Noon), "a card opened in the same tick as the removal stays hidden");
        Check(!HiddenRecents.Hides(hidden, document, Noon.AddSeconds(1)), "opening it again brings it back");
        Check(!HiddenRecents.Hides(hidden, Document(root), Noon.AddHours(-1)), "other documents are untouched");
    }

    private static void HidingSurvivesANewStoreInstance(string root)
    {
        var path = FreshStore(root);
        var document = Document(root);
        new HiddenRecents(path).Hide(document, Noon);
        Check(HiddenRecents.Hides(new HiddenRecents(path).Load(), document, Noon.AddDays(-1)), "a restart must not bring a removed card back");
    }

    private static void PathsCompareWithoutCase(string root)
    {
        var store = new HiddenRecents(FreshStore(root));
        var document = Document(root, "Mixed.PDF");
        store.Hide(document, Noon);
        Check(HiddenRecents.Hides(store.Load(), document.ToUpperInvariant(), Noon.AddDays(-1)), "Windows paths are case-insensitive");
    }

    private static void UnhideBringsItBack(string root)
    {
        var store = new HiddenRecents(FreshStore(root));
        var document = Document(root);
        store.Hide(document, Noon);
        store.Unhide(document);
        Check(!HiddenRecents.Hides(store.Load(), document, Noon.AddDays(-1)), "unhide forgets the removal");
        store.Unhide(document); // nothing left to forget: must not throw
    }

    private static void ADocumentThatNoLongerExistsIsDropped(string root)
    {
        var path = FreshStore(root);
        var store = new HiddenRecents(path);
        var gone = Document(root);
        store.Hide(gone, Noon);
        File.Delete(gone);
        store.Hide(Document(root), Noon);
        Check(!store.Load().ContainsKey(gone), "a deleted document can never show again, so its line goes");
    }

    private static void TheListIsCapped(string root)
    {
        var path = FreshStore(root);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        var oldest = Document(root);
        var lines = new List<string> { $"{Noon.AddDays(-30).Ticks}\t{oldest}" };
        for (var index = 0; index < HiddenRecents.Capacity; index++) lines.Add($"{Noon.AddMinutes(index).Ticks}\t{Document(root)}");
        File.WriteAllLines(path, lines);

        var store = new HiddenRecents(path);
        store.Hide(Document(root), Noon.AddDays(1));
        var hidden = store.Load();
        Check(hidden.Count == HiddenRecents.Capacity, $"the list kept {hidden.Count} entries");
        Check(!hidden.ContainsKey(oldest), "the oldest removal is the one dropped");
    }

    private static void MalformedLinesAreSkipped(string root)
    {
        var path = FreshStore(root);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        var document = Document(root);
        File.WriteAllLines(path, ["", "no tab here", "notanumber\tC:\\x.pdf", "99999999999999999999\tC:\\y.pdf", $"{Noon.Ticks}\t", $"{Noon.Ticks}\t{document}"]);
        var hidden = new HiddenRecents(path).Load();
        Check(hidden.Count == 1 && hidden.ContainsKey(document), "only the well-formed line counts");
    }

    private static void AFailedWriteReportsFailureAndKeepsTheOldList(string root)
    {
        var path = FreshStore(root);
        var store = new HiddenRecents(path);
        var first = Document(root);
        store.Hide(first, Noon);
        // A directory squatting on the sibling's name makes the write fail.
        Directory.CreateDirectory(path + ".partial");

        Check(!store.Hide(Document(root), Noon), "a write that cannot happen reports failure");
        var hidden = store.Load();
        Check(hidden.Count == 1 && hidden.ContainsKey(first), "the old list survives a failed write");
    }

    private static void TheDefaultPathIsLocalNotRoaming()
    {
        var local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        Check(HiddenRecents.DefaultPath == Path.Combine(local, "Vitela", "hidden-recents.txt"), $"default path was {HiddenRecents.DefaultPath}");
    }
}
