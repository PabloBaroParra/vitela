using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

/// <summary>
/// Share hands the system a copy of the session as a save would write it, without
/// that copy counting as this session saved.
/// </summary>
internal static class ShareParityTests
{
    public static async Task RunAsync()
    {
        await SharesPendingEditsWithoutMarkingSavedAsync();
        await RefusesSignatureLossUnlessAcknowledgedAsync();
        await RefusesStaleSessionsAsync();
        NamesTheSharedFileAfterTheDocument();
    }

    private static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }

    private static async Task SharesPendingEditsWithoutMarkingSavedAsync()
    {
        using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
        await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));

        var shared = await facade.ShareBytesAsync(session.SessionId);

        Check(shared is { IsSuccess: true, Value.Length: > 0 }, "share must serialize the session as a save would");
        Check((await facade.HasUnsavedChangesAsync(session.SessionId)).Value,
            "a copy sent elsewhere is not this session saved: closing must still ask about the pending edits");
    }

    private static async Task RefusesSignatureLossUnlessAcknowledgedAsync()
    {
        var core = new FakeCore { SignedDocument = true };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("signed.pdf", [1]))).Value!;

        var refused = await facade.ShareBytesAsync(session.SessionId);
        Check(!refused.IsSuccess && refused.Error!.Message.Contains("signature"),
            "an unacknowledged share must not hand out a copy with a broken signature");

        var acknowledged = await facade.ShareBytesAsync(session.SessionId, signaturesAcknowledged: true);
        Check(acknowledged.IsSuccess && core.LastSaveAcknowledgedSignatures == true, "the reader's acknowledgement must reach the core");
    }

    private static async Task RefusesStaleSessionsAsync()
    {
        using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
        var first = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
        await facade.OpenAsync(new DocumentSource("second.pdf", [2]));

        var stale = await facade.ShareBytesAsync(first.SessionId);
        Check(!stale.IsSuccess && stale.Error!.Message == "The document is no longer available.", "a retired session must not be shared");
    }

    private static void NamesTheSharedFileAfterTheDocument()
    {
        Check(ShareFileName.For("Contract.pdf") == "Contract.pdf", "a plain name passes through");
        Check(ShareFileName.For("Scan.PDF") == "Scan.PDF", "an existing PDF extension is kept, whatever its case");
        Check(ShareFileName.For("report") == "report.pdf", "the receiver needs the extension to know what it got");
        Check(ShareFileName.For("Q3: final?.pdf") == "Q3_ final_.pdf", "characters Windows forbids in a file name are replaced");
        Check(ShareFileName.For(@"..\..\evil.pdf") == ".._.._evil.pdf", "a display name never escapes the share folder");
        Check(ShareFileName.For("  ") == "document.pdf", "a blank name falls back to a readable one");
        Check(ShareFileName.For("año ñandú.pdf") == "año ñandú.pdf", "accented names are valid file names and stay as they are");
    }
}
