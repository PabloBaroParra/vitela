using Pdf.Windows.Facade;

/// <summary>
/// What crash recovery asks of the facade: what the open document looks like
/// to the autosave timer, the bytes a save would write without the session
/// counting as saved, and reopening a recovered copy as the unsaved document
/// it was.
/// </summary>
internal static class RecoveryFacadeTests
{
    public static async Task RunAsync()
    {
        await NothingOpenIsObservedAsNothingAsync();
        await ObservesUnsavedWorkAndItsRevisionAsync();
        await ObservesEncryptedContentFromTheCoreAsync();
        await SnapshotsWithoutMarkingTheSessionSavedAsync();
        await SnapshotsRecordWhetherTheyBreakASignatureAsync();
        await AnUnansweredSignatureQuestionCountsAsBreakingOneAsync();
        await RefusesToSnapshotAStaleSessionAsync();
        await ARecoveredDocumentOpensUnsavedAsync();
        await ARecoveredDocumentThatBrokeASignatureKeepsSayingSoAsync();
        await ARecoveredOpenKeepsTheUnsavedGuardAsync();
        await ClassifiesWhichFailedOpensCanNeverSucceedAsync();
    }

    private static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }

    private static PdfCoreEdit AHighlight() =>
        new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0));

    private static async Task NothingOpenIsObservedAsNothingAsync()
    {
        using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger(), conservativeLifecycle: true);

        Check(await facade.ObserveForRecoveryAsync() is null, "with no document there is nothing to protect");
    }

    private static async Task ObservesUnsavedWorkAndItsRevisionAsync()
    {
        using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger(), conservativeLifecycle: true);
        var session = (await facade.OpenAsync(new DocumentSource("quote.pdf", [1]))).Value!;

        var clean = await facade.ObserveForRecoveryAsync();
        Check(clean is { Unsaved: false, Encrypted: false } && clean.SessionId == session.SessionId, "a freshly opened file matches disk");

        await facade.EditAnnotationAsync(session.SessionId, AHighlight());
        var dirty = await facade.ObserveForRecoveryAsync();
        Check(dirty is { Unsaved: true } && dirty.EditRevision != clean!.EditRevision, "an edit is unsaved work, and moves the revision");
    }

    private static async Task ObservesEncryptedContentFromTheCoreAsync()
    {
        var core = new FakeCore();
        using var facade = new PdfDocumentFacade(core, new RecordingLogger(), conservativeLifecycle: true);
        await facade.OpenAsync(new DocumentSource("locked.pdf", [1]));

        core.EncryptedContent = true;

        Check((await facade.ObserveForRecoveryAsync())!.Encrypted, "the core's answer reaches the timer");
    }

    private static async Task SnapshotsWithoutMarkingTheSessionSavedAsync()
    {
        var core = new FakeCore();
        using var facade = new PdfDocumentFacade(core, new RecordingLogger(), conservativeLifecycle: true);
        var session = (await facade.OpenAsync(new DocumentSource("quote.pdf", [1]))).Value!;
        await facade.EditAnnotationAsync(session.SessionId, AHighlight());
        var observed = await facade.ObserveForRecoveryAsync();

        var snapshot = await facade.RecoverySnapshotAsync(session.SessionId);

        Check(snapshot is { IsSuccess: true, Value.Bytes.Length: > 0 }, "the snapshot is the bytes a save would write");
        Check(snapshot.Value!.EditRevision == observed!.EditRevision && snapshot.Value.SessionId == session.SessionId,
            "the snapshot names the revision it holds");
        Check(core.LastSaveAcknowledgedSignatures == true, "a recovery copy is not the user's document: it never stops at a signature");
        Check((await facade.HasUnsavedChangesAsync(session.SessionId)).Value, "a recovery copy is not this session saved");
    }

    private static async Task SnapshotsRecordWhetherTheyBreakASignatureAsync()
    {
        using var plain = new PdfDocumentFacade(new FakeCore(), new RecordingLogger(), conservativeLifecycle: true);
        var plainSession = (await plain.OpenAsync(new DocumentSource("plain.pdf", [1]))).Value!;
        Check(!(await plain.RecoverySnapshotAsync(plainSession.SessionId)).Value!.BreaksSignatures, "an unsigned document breaks nothing");

        using var signed = new PdfDocumentFacade(new FakeCore { SignedDocument = true }, new RecordingLogger(), conservativeLifecycle: true);
        var signedSession = (await signed.OpenAsync(new DocumentSource("signed.pdf", [1]))).Value!;
        Check((await signed.RecoverySnapshotAsync(signedSession.SessionId)).Value!.BreaksSignatures,
            "what Save would have asked is recorded, so a restored copy still asks it");
    }

    private static async Task AnUnansweredSignatureQuestionCountsAsBreakingOneAsync()
    {
        using var facade = new PdfDocumentFacade(new FakeCore { SignatureQueryFails = true }, new RecordingLogger(), conservativeLifecycle: true);
        var session = (await facade.OpenAsync(new DocumentSource("unknown.pdf", [1]))).Value!;

        var snapshot = await facade.RecoverySnapshotAsync(session.SessionId);

        Check(snapshot is { IsSuccess: true, Value.BreaksSignatures: true },
            "failing closed: reading an unknown answer as 'nothing broke' would let a later Save overwrite a signed original without asking");
    }

    private static async Task RefusesToSnapshotAStaleSessionAsync()
    {
        using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger(), conservativeLifecycle: true);
        var first = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
        await facade.OpenAsync(new DocumentSource("second.pdf", [2]));

        Check(!(await facade.RecoverySnapshotAsync(first.SessionId)).IsSuccess, "a replaced session is never written");
    }

    private static async Task ARecoveredDocumentOpensUnsavedAsync()
    {
        using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger(), conservativeLifecycle: true);

        var opened = await facade.OpenRecoveredAsync(new DocumentSource("quote.pdf", [1]), breaksSignatures: false);

        Check(opened is { Result.IsSuccess: true, Unrecoverable: false }, "the recovered bytes open");
        var session = opened.Result.Value!;
        Check(session.DisplayName == "quote.pdf", "under the name of the document that crashed");
        Check((await facade.HasUnsavedChangesAsync(session.SessionId)).Value,
            "every edit the bytes hold is unsaved, though none is in the fresh undo history");
        Check((await facade.ObserveForRecoveryAsync())!.Unsaved, "so autosave keeps protecting it");

        await facade.SaveToDestinationAsync(session.SessionId, _ => Task.CompletedTask);
        Check(!(await facade.HasUnsavedChangesAsync(session.SessionId)).Value, "until it is saved");
    }

    private static async Task ARecoveredDocumentThatBrokeASignatureKeepsSayingSoAsync()
    {
        using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger(), conservativeLifecycle: true);
        var session = (await facade.OpenRecoveredAsync(new DocumentSource("signed.pdf", [1]), breaksSignatures: true)).Result.Value!;

        Check((await facade.WillInvalidateSignaturesAsync(session.SessionId)).Value,
            "the edit that broke the signature is in the base now, where the core cannot see it; Save must still ask");
        Check((await facade.ProtectionWillInvalidateSignaturesAsync(session.SessionId)).Value, "Protect asks too");
        Check((await facade.CompressionWillInvalidateSignaturesAsync(session.SessionId)).Value, "Compress asks too");
        Check((await facade.RecoverySnapshotAsync(session.SessionId)).Value!.BreaksSignatures, "and the next snapshot records it again");
    }

    private static async Task ARecoveredOpenKeepsTheUnsavedGuardAsync()
    {
        using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger(), conservativeLifecycle: true);
        var current = (await facade.OpenAsync(new DocumentSource("current.pdf", [1]))).Value!;
        await facade.EditAnnotationAsync(current.SessionId, AHighlight());

        var refused = await facade.OpenRecoveredAsync(new DocumentSource("crashed.pdf", [1]), breaksSignatures: false);

        Check(refused is { Result.IsSuccess: false, Unrecoverable: false } && refused.Result.Error!.RequiresPendingEditDecision,
            "a restore never silently replaces unsaved work, and the entry is kept for later");
    }

    private static async Task ClassifiesWhichFailedOpensCanNeverSucceedAsync()
    {
        foreach (var (error, unrecoverable) in new[]
        {
            // Autosave never writes an encrypted copy: bytes that want a password are not one it wrote.
            (PdfCoreError.PasswordRequired, true),
            (PdfCoreError.WrongPassword, true),
            (PdfCoreError.UnsupportedSecurityHandler, true),
            // The parser refused the bytes themselves.
            (PdfCoreError.Internal, true),
            // The renderer or the disk: worth asking about again next launch.
            (PdfCoreError.RenderFailed, false),
            (PdfCoreError.Io, false),
        })
        {
            using var facade = new PdfDocumentFacade(new FakeCore { OpenError = error }, new RecordingLogger(), conservativeLifecycle: true);
            var opened = await facade.OpenRecoveredAsync(new DocumentSource("broken.pdf", [1]), breaksSignatures: false);
            Check(!opened.Result.IsSuccess && opened.Unrecoverable == unrecoverable, $"{error} classified as unrecoverable={unrecoverable}");
        }
    }
}
