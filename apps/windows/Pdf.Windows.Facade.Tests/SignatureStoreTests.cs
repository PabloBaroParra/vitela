using Pdf.Windows.Facade;

/// <summary>The remembered signature: one PNG, written beside itself and moved over. Mirrors Android's `FileSignatureStoreTest`.</summary>
internal static class SignatureStoreTests
{
    public static Task RunAsync()
    {
        var root = Path.Combine(Path.GetTempPath(), "vitela-signature-" + Guid.NewGuid().ToString("N"));
        try
        {
            AMissingFileLoadsAsNothing(root);
            SaveThenLoadRoundTrips(root);
            SavingReplacesTheRememberedSignature(root);
            AnEmptyFileCountsAsNothing(root);
            DeleteForgetsItAndToleratesNothingToDelete(root);
            AFailedWriteKeepsTheOldSignature(root);
            TheDefaultPathIsLocalNotRoaming();
            TheNoOpStoreRemembersNothing();
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

    private static string Fresh(string root) => Path.Combine(root, Guid.NewGuid().ToString("N"), "signature.png");

    private static void AMissingFileLoadsAsNothing(string root)
    {
        Check(new FileSignatureStore(Fresh(root)).Load() is null, "a missing file has no signature");
    }

    private static void SaveThenLoadRoundTrips(string root)
    {
        var path = Fresh(root); // its folder does not exist yet: the store creates it
        var store = new FileSignatureStore(path);
        byte[] png = [137, 80, 78, 71, 1, 2, 3];

        Check(store.Save(png), "save succeeds");
        Check(store.Load()!.SequenceEqual(png), "load returns what was saved");
        Check(!File.Exists(path + ".partial"), "the sibling is gone after the move");
    }

    private static void SavingReplacesTheRememberedSignature(string root)
    {
        var path = Fresh(root);
        var store = new FileSignatureStore(path);
        Check(store.Save([1, 2, 3]) && store.Save([9, 8]), "both saves succeed");
        Check(store.Load()!.SequenceEqual(new byte[] { 9, 8 }), "the second signature replaces the first");
        Check(Directory.GetFiles(Path.GetDirectoryName(path)!).Length == 1, "only the signature file remains");
    }

    private static void AnEmptyFileCountsAsNothing(string root)
    {
        var path = Fresh(root);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllBytes(path, []);
        Check(new FileSignatureStore(path).Load() is null, "an empty file is not a signature");
    }

    private static void DeleteForgetsItAndToleratesNothingToDelete(string root)
    {
        var path = Fresh(root);
        var store = new FileSignatureStore(path);
        store.Delete(); // nothing there yet: must not throw
        store.Save([1]);
        store.Delete();
        Check(!File.Exists(path) && store.Load() is null, "delete removes the file");
    }

    private static void AFailedWriteKeepsTheOldSignature(string root)
    {
        var path = Fresh(root);
        var store = new FileSignatureStore(path);
        store.Save([1, 2, 3]);
        // A directory squatting on the sibling's name makes the write fail.
        Directory.CreateDirectory(path + ".partial");

        Check(!store.Save([7, 7, 7]), "a write that cannot happen reports failure");
        Check(store.Load()!.SequenceEqual(new byte[] { 1, 2, 3 }), "the old signature survives a failed save");
    }

    private static void TheDefaultPathIsLocalNotRoaming()
    {
        var local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        var roaming = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
        var path = FileSignatureStore.DefaultPath;
        Check(path == Path.Combine(local, "Vitela", "signature.png"), $"default path was {path}");
        Check(!path.StartsWith(roaming + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase)
            || local.StartsWith(roaming, StringComparison.OrdinalIgnoreCase), "the signature must not roam");
    }

    private static void TheNoOpStoreRemembersNothing()
    {
        ISignatureStore store = NoSignatureStore.Instance;
        Check(!store.Save([1]), "the no-op store reports it kept nothing");
        Check(store.Load() is null, "and loads nothing");
        store.Delete();
    }
}
