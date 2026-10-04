using Pdf.Windows.Facade;

internal static class SelectionParityTests
{
    public static async Task RunAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        var permitted = false;
        var core = new FakeCore { ExtractionPermissionOverride = () => permitted, ContentEditingPermitted = false };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("restricted.pdf", [1]))).Value!;
        Check(!facade.SelectionAllowed(session.SessionId).IsSuccess, "Selection must refuse extraction independently of editing");
        var denied = await facade.PageCharactersAsync(session.SessionId, 0);
        Check(!denied.IsSuccess && denied.Error!.Message == "This document does not permit copying or extracting its text.", "Specific selection refusal");
        Check(core.PageCharactersCalls.IsEmpty, "Refused selection must not extract characters");
        permitted = true;
        Check(facade.SelectionAllowed(session.SessionId).IsSuccess, "Read-only editing must not refuse selection");
        var loaded = await facade.PageCharactersAsync(session.SessionId, 0);
        Check(loaded.IsSuccess && core.PageCharactersCalls.Count == 1, "Permitted selection loads characters");
        loaded.Value!.Dispose();
        permitted = false;
        Check(!facade.SelectionAllowed(session.SessionId).IsSuccess, "Copy must recheck permission even with cached characters");
        Check(!facade.SelectionAllowed("stale").IsSuccess, "Stale clipboard target must be refused");
    }
}
