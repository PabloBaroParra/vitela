using Pdf.Windows.Recovery;
using System.Runtime.Versioning;
using System.Security.AccessControl;
using System.Security.Principal;

/// <summary>
/// The recovery directory on disk, its sidecar format, the per-tick rule and
/// the failure backoff — everything about crash recovery that needs no window.
/// </summary>
internal static class RecoveryStoreTests
{
    public static Task RunAsync()
    {
        var root = Path.Combine(Path.GetTempPath(), "vitela-recovery-store-" + Guid.NewGuid().ToString("N"));
        try
        {
            ASidecarRoundTrips();
            TheSignatureFlagRoundTripsAndFailsClosed();
            UnknownAndMalformedLinesAreSkipped();
            ASidecarMissingARequiredFieldIsRefused();
            ADifferentHeaderIsADifferentFormat();
            TheDefaultDirectoryIsLocalNotRoaming();
            AWrittenEntryIsOfferedOnceItsOwnerIsGone(Fresh(root));
            AnEntryAnotherWindowHoldsIsLeftAlone(Fresh(root));
            AClearedSlotLeavesNothingBehind(Fresh(root));
            ARewriteReplacesTheEntryWhole(Fresh(root));
            IncompleteEntriesAreDiscardedWithALogLine(Fresh(root));
            LeftoverTemporariesAreSweptAndTheEntryStillOffered(Fresh(root));
            OrphansAreOfferedNewestFirst(Fresh(root));
            ADiscardedOrphanIsGoneAndAReleasedOneComesBack(Fresh(root));
            ReadingBytesThatAreNotAPdfIsUnrecoverable(Fresh(root));
            FilesThatAreNotEntriesAreLeftAlone(Fresh(root));
            if (OperatingSystem.IsWindows()) TheDirectoryIsPrivateToTheUser(Fresh(root));
            PolicyRules();
            BackoffRules();
        }
        finally
        {
            // Best effort: a failed check may leave a lock open, and its message matters more.
            try { if (Directory.Exists(root)) Directory.Delete(root, recursive: true); }
            catch (IOException) { }
        }
        return Task.CompletedTask;
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static RecoveryDirectory Fresh(string root) => new(Path.Combine(root, Guid.NewGuid().ToString("N"), "recovery"));

    internal static readonly byte[] PdfBytes = "%PDF-1.7\nnot really a document\n%%EOF\n"u8.ToArray();

    private static RecoveryMeta AMeta(long savedAt = 1_700_000_000) =>
        new(savedAt, "quote.pdf", @"C:\Users\someone\Documents\quote.pdf", BreaksSignatures: false);

    private static List<string> Logged(out Action<string> log)
    {
        var lines = new List<string>();
        log = lines.Add;
        return lines;
    }

    private static void ASidecarRoundTrips()
    {
        var meta = AMeta();
        Check(RecoveryMeta.Parse(meta.Encode()) == meta, "a sidecar round-trips");
        var odd = meta with { Name = "Q3: 50% final\tdraft\n.pdf", Origin = @"C:\Users\some one\Año 50%\quote #1.pdf" };
        Check(RecoveryMeta.Parse(odd.Encode()) == odd, "a name and an origin with separators, spaces and accents round-trip");
        var none = meta with { Origin = null };
        Check(RecoveryMeta.Parse(none.Encode()) == none, "a document with no file round-trips");
    }

    private static void TheSignatureFlagRoundTripsAndFailsClosed()
    {
        var meta = AMeta() with { BreaksSignatures = true };
        Check(RecoveryMeta.Parse(meta.Encode()) == meta, "the flag round-trips");
        Check(RecoveryMeta.Parse("vitela-recovery 1\nsaved\t5\nname\tq\n")!.BreaksSignatures == false,
            "a sidecar written before the flag existed broke nothing");
        bool Flag(string value) => RecoveryMeta.Parse($"vitela-recovery 1\nsaved\t5\nname\tq\nbreaks-signatures\t{value}\n")!.BreaksSignatures;
        Check(!Flag("0"), "only an explicit 0 clears the flag");
        Check(Flag("1") && Flag("yes") && Flag(""), "anything else may be a later version saying a signature broke");
    }

    private static void UnknownAndMalformedLinesAreSkipped()
    {
        var meta = RecoveryMeta.Parse("vitela-recovery 1\nsaved\t5\nno tab on this line\nlater-field\tvalue\norigin\tnot a uri\nname\tquote.pdf\n");
        Check(meta is { SavedAt: 5, Name: "quote.pdf", Origin: null }, "a line this version cannot read costs that line, never the entry");
        Check(RecoveryMeta.Parse("vitela-recovery 1\r\nsaved\t5\r\nname\tquote.pdf\r\n") is { Name: "quote.pdf" }, "CRLF line ends read too");
    }

    private static void ASidecarMissingARequiredFieldIsRefused()
    {
        Check(RecoveryMeta.Parse("vitela-recovery 1\nname\tquote.pdf\n") is null, "no time");
        Check(RecoveryMeta.Parse("vitela-recovery 1\nsaved\t5\n") is null, "no name");
        Check(RecoveryMeta.Parse("vitela-recovery 1\nsaved\tsoon\nname\tquote.pdf\n") is null, "a time that does not parse is missing");
        Check(RecoveryMeta.Parse("") is null, "an empty sidecar");
    }

    private static void ADifferentHeaderIsADifferentFormat() =>
        Check(RecoveryMeta.Parse("vitela-recovery 2\nsaved\t5\nname\tq\n") is null, "a later format is not guessed at");

    private static void TheDefaultDirectoryIsLocalNotRoaming()
    {
        var local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        Check(RecoveryDirectory.DefaultRoot == Path.Combine(local, "Vitela", "recovery"), "beside the other per-user Vitela files, never roaming");
    }

    private static void AWrittenEntryIsOfferedOnceItsOwnerIsGone(RecoveryDirectory dir)
    {
        var slot = dir.OpenSlot(EntryKey.Fresh());
        slot.Paths.Write(PdfBytes, AMeta());

        Check(dir.Orphans(_ => { }).Count == 0, "a window that is still open owns its entry");

        slot.ReleaseLikeACrash();
        var orphans = dir.Orphans(_ => { });
        Check(orphans.Count == 1 && orphans[0].Meta == AMeta(), "a crashed window's entry is offered with its sidecar");
        Check(orphans[0].ReadPdf().SequenceEqual(PdfBytes), "with the bytes it wrote");
        foreach (var orphan in orphans) orphan.Dispose();
    }

    private static void AnEntryAnotherWindowHoldsIsLeftAlone(RecoveryDirectory dir)
    {
        var slot = dir.OpenSlot(EntryKey.Fresh());
        slot.Paths.Write(PdfBytes, AMeta());
        var claimed = dir.Orphans(_ => { });
        Check(claimed.Count == 0, "held");
        Check(File.Exists(slot.Paths.PdfPath) && File.Exists(slot.Paths.MetaPath), "and untouched");
        slot.Clear();
    }

    private static void AClearedSlotLeavesNothingBehind(RecoveryDirectory dir)
    {
        var slot = dir.OpenSlot(EntryKey.Fresh());
        slot.Paths.Write(PdfBytes, AMeta());

        slot.Clear();

        Check(Directory.GetFiles(dir.Root).Length == 0, "a save, close or discard deletes the entry, lock included");
        Check(dir.Orphans(_ => { }).Count == 0, "and nothing is offered");
    }

    private static void ARewriteReplacesTheEntryWhole(RecoveryDirectory dir)
    {
        var slot = dir.OpenSlot(EntryKey.Fresh());
        slot.Paths.Write(PdfBytes, AMeta(1));
        byte[] newer = [.. PdfBytes, .. "more"u8.ToArray()];

        slot.Paths.Write(newer, AMeta(2));
        slot.ReleaseLikeACrash();

        var orphan = dir.Orphans(_ => { }).Single();
        Check(orphan.Meta.SavedAt == 2 && orphan.ReadPdf().SequenceEqual(newer), "the second snapshot replaced the first");
        Check(Directory.GetFiles(dir.Root).All(file => !file.EndsWith(".tmp")), "no sibling is left behind");
        orphan.Discard();
    }

    private static void IncompleteEntriesAreDiscardedWithALogLine(RecoveryDirectory dir)
    {
        // Bytes with no sidecar: a crash between the two renames.
        var noSidecar = dir.OpenSlot(EntryKey.Fresh());
        File.WriteAllBytes(noSidecar.Paths.PdfPath, PdfBytes);
        noSidecar.ReleaseLikeACrash();
        // A sidecar that does not parse.
        var badSidecar = dir.OpenSlot(EntryKey.Fresh());
        badSidecar.Paths.Write(PdfBytes, AMeta());
        File.WriteAllText(badSidecar.Paths.MetaPath, "garbage");
        badSidecar.ReleaseLikeACrash();
        // A sidecar vouching for bytes that are gone.
        var noBytes = dir.OpenSlot(EntryKey.Fresh());
        noBytes.Paths.Write(PdfBytes, AMeta());
        File.Delete(noBytes.Paths.PdfPath);
        noBytes.ReleaseLikeACrash();
        var lines = Logged(out var log);

        Check(dir.Orphans(log).Count == 0, "nothing incomplete is offered");
        Check(lines.Count == 3, "each discard says so in the log");
        Check(Directory.GetFiles(dir.Root).Length == 0, "and nothing incomplete piles up");
    }

    private static void LeftoverTemporariesAreSweptAndTheEntryStillOffered(RecoveryDirectory dir)
    {
        var slot = dir.OpenSlot(EntryKey.Fresh());
        slot.Paths.Write(PdfBytes, AMeta());
        var leftover = Path.Combine(dir.Root, "." + Path.GetFileName(slot.Paths.PdfPath) + ".tmp");
        File.WriteAllBytes(leftover, [1, 2, 3]);
        slot.ReleaseLikeACrash();

        var orphan = dir.Orphans(_ => { }).Single();

        Check(!File.Exists(leftover), "a write a crash cut short is swept");
        Check(orphan.ReadPdf().SequenceEqual(PdfBytes), "and the previous complete entry is offered");
        orphan.Discard();
    }

    private static void OrphansAreOfferedNewestFirst(RecoveryDirectory dir)
    {
        foreach (var savedAt in new long[] { 10, 30, 20 })
        {
            var slot = dir.OpenSlot(EntryKey.Fresh());
            slot.Paths.Write(PdfBytes, AMeta(savedAt));
            slot.ReleaseLikeACrash();
        }

        var orphans = dir.Orphans(_ => { });

        Check(orphans.Select(orphan => orphan.Meta.SavedAt).SequenceEqual([30L, 20L, 10L]), "newest first");
        foreach (var orphan in orphans) orphan.Discard();
    }

    private static void ADiscardedOrphanIsGoneAndAReleasedOneComesBack(RecoveryDirectory dir)
    {
        var slot = dir.OpenSlot(EntryKey.Fresh());
        slot.Paths.Write(PdfBytes, AMeta());
        slot.ReleaseLikeACrash();

        var claimed = dir.Orphans(_ => { }).Single();
        Check(dir.Orphans(_ => { }).Count == 0, "a claimed orphan is held: no other instance offers it meanwhile");
        claimed.Dispose();
        var again = dir.Orphans(_ => { }).Single();
        Check(again.Meta == AMeta(), "'later' releases the claim and keeps the files");

        again.Discard();
        Check(Directory.GetFiles(dir.Root).Length == 0 && dir.Orphans(_ => { }).Count == 0, "a discard deletes it");
    }

    private static void ReadingBytesThatAreNotAPdfIsUnrecoverable(RecoveryDirectory dir)
    {
        var slot = dir.OpenSlot(EntryKey.Fresh());
        slot.Paths.Write("not a pdf"u8.ToArray(), AMeta());
        slot.ReleaseLikeACrash();
        var orphan = dir.Orphans(_ => { }).Single();

        Exception? failure = null;
        try { orphan.ReadPdf(); }
        catch (Exception error) { failure = error; }

        Check(failure is not null && RecoveryOrphan.IsUnrecoverableReadFailure(failure), "bytes that are not a PDF never will be");
        Check(!RecoveryOrphan.IsUnrecoverableReadFailure(new IOException("disk trouble")), "a disk failing to hand them over says nothing about them");
        Check(!RecoveryOrphan.IsUnrecoverableReadFailure(new UnauthorizedAccessException()), "nor does a permission failure");
        orphan.Discard();
    }

    private static void FilesThatAreNotEntriesAreLeftAlone(RecoveryDirectory dir)
    {
        dir.OpenSlot(EntryKey.Fresh()).Clear();
        var stranger = Path.Combine(dir.Root, "notes.txt");
        var lookalike = Path.Combine(dir.Root, "report.pdf");
        File.WriteAllText(stranger, "mine");
        File.WriteAllBytes(lookalike, PdfBytes);

        Check(dir.Orphans(_ => { }).Count == 0, "only entry-shaped names are entries");
        Check(File.Exists(stranger) && File.Exists(lookalike), "and nothing else is touched");
        File.Delete(stranger);
        File.Delete(lookalike);
    }

    [SupportedOSPlatform("windows")]
    private static void TheDirectoryIsPrivateToTheUser(RecoveryDirectory dir)
    {
        var slot = dir.OpenSlot(EntryKey.Fresh());
        slot.Paths.Write(PdfBytes, AMeta());

        var security = new DirectoryInfo(dir.Root).GetAccessControl();
        var rules = security.GetAccessRules(includeExplicit: true, includeInherited: true, typeof(SecurityIdentifier)).Cast<FileSystemAccessRule>().ToList();
        var me = WindowsIdentity.GetCurrent().User!;

        Check(security.AreAccessRulesProtected, "the directory does not inherit the profile's wider rules");
        Check(rules.Count > 0 && rules.All(rule => rule.IdentityReference.Equals(me)), "only the current user is granted anything");
        var fileRules = new FileInfo(slot.Paths.PdfPath).GetAccessControl().GetAccessRules(true, true, typeof(SecurityIdentifier)).Cast<FileSystemAccessRule>();
        Check(fileRules.All(rule => rule.IdentityReference.Equals(me)), "and the files inherit that");
        slot.Clear();
    }

    private static readonly RecoveryRevision Revision = new("session", 3);

    private static Pdf.Windows.Facade.RecoveryObservation Dirty(bool unsaved = true, bool encrypted = false) => new("session", 3, unsaved, encrypted);

    private static void PolicyRules()
    {
        Check(AutosavePolicy.Decide(Dirty(), editingText: false, written: null) == AutosaveDecision.Write, "unsaved work not yet on disk is written");
        Check(AutosavePolicy.Decide(Dirty(), false, Revision) == AutosaveDecision.Wait, "an unchanged document is not written again");
        Check(AutosavePolicy.Decide(Dirty(), false, Revision with { SessionId = "other" }) == AutosaveDecision.Write, "a new session is a change");
        Check(AutosavePolicy.Decide(Dirty(), false, Revision with { EditRevision = 2 }) == AutosaveDecision.Write, "a new edit is a change");
        Check(AutosavePolicy.Decide(Dirty(unsaved: false), false, Revision) == AutosaveDecision.Forget, "a document that matches disk forgets its entry");
        Check(AutosavePolicy.Decide(null, false, Revision) == AutosaveDecision.Forget, "no document forgets the entry");
        Check(AutosavePolicy.Decide(Dirty(encrypted: true), false, null) == AutosaveDecision.Forget, "an encrypted document is never written");
        Check(AutosavePolicy.Decide(Dirty(), editingText: true, null) == AutosaveDecision.Wait, "an open retype box postpones the snapshot");
        Check(AutosavePolicy.Decide(Dirty(), editingText: true, Revision) == AutosaveDecision.Wait, "and never forgets for it");
    }

    private static void BackoffRules()
    {
        var first = AutosaveBackoff.FirstDelay;
        Check(AutosaveBackoff.Delay(1) == first && AutosaveBackoff.Delay(2) == first * 2 && AutosaveBackoff.Delay(3) == first * 4, "the delay doubles from one interval");
        Check(AutosaveBackoff.Delay(5) == AutosaveBackoff.MaxDelay && AutosaveBackoff.Delay(int.MaxValue) == AutosaveBackoff.MaxDelay, "and stops at the cap without overflowing");

        var start = TimeSpan.FromHours(1);
        var backoff = new AutosaveBackoff();
        Check(backoff.Ready(start), "nothing failed means ready");
        Check(backoff.Failed(start), "the first failure of a streak is reported");
        Check(!backoff.Ready(start) && !backoff.Ready(start + first - TimeSpan.FromSeconds(1)) && backoff.Ready(start + first), "held back for one interval");
        Check(!backoff.Failed(start + first), "later failures of a streak are not reported again");
        Check(!backoff.Ready(start + first * 2) && backoff.Ready(start + first * 3), "the second waits twice as long");

        backoff.Succeeded();
        Check(backoff.Ready(start) && backoff.Failed(start), "a success ends the streak; the next one is reported again");
    }
}
