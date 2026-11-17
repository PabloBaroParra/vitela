using Pdf.Windows.Facade;
using Pdf.Windows.Recovery;

/// <summary>
/// The autosave timer's behaviour, driven tick by tick against a real
/// directory and a scripted document — including the races a forget can lose.
/// </summary>
internal static class AutosaveSchedulerTests
{
    public static async Task RunAsync()
    {
        var root = Path.Combine(Path.GetTempPath(), "vitela-autosave-" + Guid.NewGuid().ToString("N"));
        try
        {
            await WritesUnsavedWorkAndOffersItAfterACrashAsync(Fresh(root));
            await DoesNotWriteAnUnchangedDocumentAgainAsync(Fresh(root));
            await ForgetsTheEntryOnceTheDocumentMatchesDiskAsync(Fresh(root));
            await NeverWritesAnEncryptedDocumentAsync(Fresh(root));
            await AnOpenRetypeBoxPostponesTheSnapshotAsync(Fresh(root));
            await ACleanShutdownLeavesNothingToOfferAsync(Fresh(root));
            await AForgetDuringTheSnapshotWritesNothingAsync(Fresh(root));
            await AForgetJustBeforeTheDiskWriteCannotResurrectTheEntryAsync(Fresh(root));
            await AShutdownWaitsForAWriteInFlightAsync(Fresh(root));
            await AFailedWriteBacksOffAndNotifiesOnceAsync(Fresh(root));
            await AFailedSnapshotBacksOffTooAsync(Fresh(root));
            await AForgetEndsTheFailureStreakAsync(Fresh(root));
            await AnAdoptedEntryIsKeptUntilTheWindowHasWrittenItsOwnAsync(Fresh(root));
            await RecordsWhatTheSidecarNeedsAsync(Fresh(root));
            await NoDirectoryMeansNothingIsEverWrittenAsync();
        }
        finally
        {
            // Best effort: a failed check may leave a lock open, and its message matters more.
            try { if (Directory.Exists(root)) Directory.Delete(root, recursive: true); }
            catch (IOException) { }
        }
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static RecoveryDirectory Fresh(string root) => new(Path.Combine(root, Guid.NewGuid().ToString("N"), "recovery"));

    private static readonly TimeSpan Now = TimeSpan.FromHours(1);

    private sealed class ScriptedDocument : IAutosaveSource
    {
        public RecoveryObservation? Observed { get; set; } = new("session", 1, Unsaved: true, Encrypted: false);
        public bool EditingText { get; set; }
        public bool SnapshotFails { get; set; }
        public bool BreaksSignatures { get; set; }
        public string Name { get; set; } = "quote.pdf";
        public string? Origin { get; set; } = @"C:\Docs\quote.pdf";
        public Func<Task>? DuringSnapshot { get; set; }
        public byte[] Bytes => [.. RecoveryStoreTests.PdfBytes, .. BitConverter.GetBytes(Observed?.EditRevision ?? 0)];

        public Task<RecoveryObservation?> ObserveAsync() => Task.FromResult(Observed);

        public async Task<OperationResult<RecoverySnapshot>> SnapshotAsync(string sessionId)
        {
            await Task.Yield();
            if (DuringSnapshot is { } during) await during();
            if (SnapshotFails) return OperationResult<RecoverySnapshot>.Failure(new UserSafeError("The document could not be processed.", "x"));
            return OperationResult<RecoverySnapshot>.Success(new RecoverySnapshot(sessionId, Observed!.EditRevision, Bytes, BreaksSignatures));
        }

        public (string Name, string? Origin) Describe() => (Name, Origin);

        public void Edit() => Observed = Observed! with { EditRevision = Observed.EditRevision + 1 };
    }

    private sealed class Notices
    {
        public int Count;
        public List<string> Log { get; } = [];
    }

    private static AutosaveScheduler Scheduler(RecoveryDirectory dir, ScriptedDocument document, Notices? notices = null)
    {
        notices ??= new Notices();
        return new AutosaveScheduler(dir, document, notices.Log.Add, () => notices.Count++);
    }

    private static bool HasData(RecoveryDirectory dir) =>
        Directory.Exists(dir.Root) && Directory.EnumerateFiles(dir.Root).Any(file => file.EndsWith(".pdf") || file.EndsWith(".meta"));

    private static async Task WritesUnsavedWorkAndOffersItAfterACrashAsync(RecoveryDirectory dir)
    {
        var document = new ScriptedDocument();
        var autosave = Scheduler(dir, document);

        await autosave.TickAsync(Now);
        Check(HasData(dir) && dir.Orphans(_ => { }).Count == 0, "the entry is written, and held while the window lives");

        autosave.ReleaseLikeACrash();
        var orphan = dir.Orphans(_ => { }).Single();
        Check(orphan.ReadPdf().SequenceEqual(document.Bytes), "after a crash the next launch finds the bytes a save would have written");
        orphan.Discard();
    }

    private static async Task DoesNotWriteAnUnchangedDocumentAgainAsync(RecoveryDirectory dir)
    {
        var document = new ScriptedDocument();
        var autosave = Scheduler(dir, document);

        await autosave.TickAsync(Now);
        await autosave.TickAsync(Now);
        Check(autosave.Attempts == 1, "nothing moved, nothing is serialized");

        document.Edit();
        await autosave.TickAsync(Now);
        Check(autosave.Attempts == 2, "an edit is written on the next tick");
        autosave.Shutdown(TimeSpan.FromSeconds(5));
    }

    private static async Task ForgetsTheEntryOnceTheDocumentMatchesDiskAsync(RecoveryDirectory dir)
    {
        var document = new ScriptedDocument();
        var autosave = Scheduler(dir, document);
        await autosave.TickAsync(Now);

        document.Observed = document.Observed! with { Unsaved = false };
        await autosave.TickAsync(Now);

        Check(Directory.GetFiles(dir.Root).Length == 0, "the first tick that finds nothing unsaved deletes the entry");
        document.Observed = null;
        await autosave.TickAsync(Now);
        Check(Directory.GetFiles(dir.Root).Length == 0, "and no document keeps it deleted");
    }

    private static async Task NeverWritesAnEncryptedDocumentAsync(RecoveryDirectory dir)
    {
        var document = new ScriptedDocument();
        var autosave = Scheduler(dir, document);
        await autosave.TickAsync(Now);

        document.Observed = document.Observed! with { Encrypted = true, EditRevision = 9 };
        await autosave.TickAsync(Now);

        Check(Directory.GetFiles(dir.Root).Length == 0, "an imported protected page deletes the entry instead of rewriting it");
        Check(autosave.Attempts == 1, "and is never serialized");
    }

    private static async Task AnOpenRetypeBoxPostponesTheSnapshotAsync(RecoveryDirectory dir)
    {
        var document = new ScriptedDocument { EditingText = true };
        var autosave = Scheduler(dir, document);

        await autosave.TickAsync(Now);
        Check(autosave.Attempts == 0 && !HasData(dir), "the box is never committed by the timer");

        document.EditingText = false;
        await autosave.TickAsync(Now);
        Check(autosave.Attempts == 1 && HasData(dir), "the next tick after it closes writes");
        autosave.Shutdown(TimeSpan.FromSeconds(5));
    }

    private static async Task ACleanShutdownLeavesNothingToOfferAsync(RecoveryDirectory dir)
    {
        var document = new ScriptedDocument();
        var autosave = Scheduler(dir, document);
        await autosave.TickAsync(Now);

        autosave.Shutdown(TimeSpan.FromSeconds(5));
        await autosave.TickAsync(Now);

        Check(Directory.GetFiles(dir.Root).Length == 0, "a clean close (after Save or Discard) deletes the entry, and no later tick writes one");
    }

    private static async Task AForgetDuringTheSnapshotWritesNothingAsync(RecoveryDirectory dir)
    {
        var document = new ScriptedDocument();
        AutosaveScheduler? autosave = null;
        document.DuringSnapshot = () => { autosave!.Forget(); return Task.CompletedTask; };
        autosave = Scheduler(dir, document);

        await autosave.TickAsync(Now);

        Check(!HasData(dir), "a document saved while it was being serialized is not written afterwards");
        Check(autosave.Written is null, "and nothing is recorded as written");
    }

    /// <summary>
    /// The race the worker's post-write check exists for: the forget lands after
    /// the worker's last pre-write check but before its files are in place.
    /// Fails when that check is removed.
    /// </summary>
    private static async Task AForgetJustBeforeTheDiskWriteCannotResurrectTheEntryAsync(RecoveryDirectory dir)
    {
        var document = new ScriptedDocument();
        var autosave = Scheduler(dir, document);
        using var reached = new ManualResetEventSlim();
        using var release = new ManualResetEventSlim();
        autosave.BeforeDiskWrite = () => { reached.Set(); release.Wait(TimeSpan.FromSeconds(10)); };

        var tick = autosave.TickAsync(Now);
        Check(reached.Wait(TimeSpan.FromSeconds(10)), "the worker reaches the disk write");
        autosave.Forget();
        release.Set();
        await tick;

        Check(!HasData(dir), "the late write deletes what it wrote");
        Check(dir.Orphans(_ => { }).Count == 0, "and nothing is offered at the next launch");
    }

    private static async Task AShutdownWaitsForAWriteInFlightAsync(RecoveryDirectory dir)
    {
        var document = new ScriptedDocument();
        var autosave = Scheduler(dir, document);
        using var reached = new ManualResetEventSlim();
        autosave.BeforeDiskWrite = () => { reached.Set(); Thread.Sleep(200); };

        var tick = autosave.TickAsync(Now);
        Check(reached.Wait(TimeSpan.FromSeconds(10)), "the worker reaches the disk write");
        autosave.Shutdown(TimeSpan.FromSeconds(5));

        Check(!HasData(dir), "the exit waited for the worker to take its files back");
        await tick;
    }

    private static async Task AFailedWriteBacksOffAndNotifiesOnceAsync(RecoveryDirectory dir)
    {
        // A file where the directory should be: every write fails.
        Directory.CreateDirectory(Path.GetDirectoryName(dir.Root)!);
        File.WriteAllText(dir.Root, "in the way");
        var document = new ScriptedDocument();
        var notices = new Notices();
        var autosave = Scheduler(dir, document, notices);

        await autosave.TickAsync(Now);
        Check(notices.Count == 1 && notices.Log.Count == 1, "the first failure is reported, and logged");

        document.Edit();
        await autosave.TickAsync(Now + TimeSpan.FromSeconds(30));
        Check(notices.Log.Count == 1, "within the backoff nothing is attempted");

        await autosave.TickAsync(Now + AutosaveBackoff.FirstDelay);
        Check(notices.Count == 1 && notices.Log.Count == 2, "after it, a retry fails again — logged, not reported again");

        File.Delete(dir.Root);
        await autosave.TickAsync(Now + AutosaveBackoff.FirstDelay * 3);
        Check(HasData(dir), "once the disk is back the entry is written");
        autosave.Shutdown(TimeSpan.FromSeconds(5));
    }

    private static async Task AFailedSnapshotBacksOffTooAsync(RecoveryDirectory dir)
    {
        var document = new ScriptedDocument { SnapshotFails = true };
        var notices = new Notices();
        var autosave = Scheduler(dir, document, notices);

        await autosave.TickAsync(Now);
        document.Edit();
        await autosave.TickAsync(Now + TimeSpan.FromSeconds(1));

        Check(autosave.Attempts == 1 && notices.Count == 1, "a serialization failure holds the next attempt back");
    }

    private static async Task AForgetEndsTheFailureStreakAsync(RecoveryDirectory dir)
    {
        var document = new ScriptedDocument { SnapshotFails = true };
        var notices = new Notices();
        var autosave = Scheduler(dir, document, notices);
        await autosave.TickAsync(Now);

        autosave.Forget();
        document.SnapshotFails = false;
        await autosave.TickAsync(Now + TimeSpan.FromSeconds(1));

        Check(HasData(dir), "the next document is written without waiting out the last one's backoff");
        autosave.Shutdown(TimeSpan.FromSeconds(5));
    }

    private static async Task AnAdoptedEntryIsKeptUntilTheWindowHasWrittenItsOwnAsync(RecoveryDirectory dir)
    {
        var crashed = Scheduler(dir, new ScriptedDocument { Name = "crashed.pdf" });
        await crashed.TickAsync(Now);
        crashed.ReleaseLikeACrash();
        var orphan = dir.Orphans(_ => { }).Single();

        var document = new ScriptedDocument { Name = "crashed.pdf", EditingText = true };
        var autosave = Scheduler(dir, document);
        autosave.Adopt(orphan);
        await autosave.TickAsync(Now);
        Check(Directory.GetFiles(dir.Root, "*.pdf").Length == 1, "until this window has written its own entry, the crashed one stays");

        document.EditingText = false;
        await autosave.TickAsync(Now);
        Check(Directory.GetFiles(dir.Root, "*.pdf").Length == 1 && Directory.GetFiles(dir.Root, "*.lock").Length == 1,
            "then it is deleted: the restored document lives in this window's entry");

        autosave.ReleaseLikeACrash();
        var again = dir.Orphans(_ => { }).Single();
        Check(again.Meta.Name == "crashed.pdf", "a second crash still has the work to offer");
        again.Discard();
    }

    private static async Task RecordsWhatTheSidecarNeedsAsync(RecoveryDirectory dir)
    {
        var document = new ScriptedDocument { BreaksSignatures = true, Name = "contract.pdf", Origin = @"D:\Signed\contract.pdf" };
        var autosave = Scheduler(dir, document);
        var before = DateTimeOffset.UtcNow.ToUnixTimeSeconds();

        await autosave.TickAsync(Now);
        autosave.ReleaseLikeACrash();

        var orphan = dir.Orphans(_ => { }).Single();
        Check(orphan.Meta is { Name: "contract.pdf", Origin: @"D:\Signed\contract.pdf", BreaksSignatures: true }, "name, origin and the signature answer");
        Check(orphan.Meta.SavedAt >= before, "and when");
        orphan.Discard();
    }

    private static async Task NoDirectoryMeansNothingIsEverWrittenAsync()
    {
        var document = new ScriptedDocument();
        var autosave = new AutosaveScheduler(null, document, _ => { }, () => { });

        await autosave.TickAsync(Now);

        Check(autosave.Attempts == 0, "a disabled scheduler never serializes");
    }
}
