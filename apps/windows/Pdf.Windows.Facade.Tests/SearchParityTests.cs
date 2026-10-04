using Pdf.Windows.Facade;

internal static class SearchParityTests
{
    public static async Task RunAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        var core = new FakeCore { ExtractionPermitted = false };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("restricted.pdf", [1]))).Value!;
        var refused = await facade.SearchAsync(session.SessionId, "secret");
        Check(!refused.IsSuccess && refused.Error!.Message == "This document does not permit copying or extracting its text.", "Specific extraction refusal");
        Check(core.SearchQueries.IsEmpty, "Forbidden search must not queue a matcher or expose counts");
        var allowedCore = new FakeCore { ContentEditingPermitted = false };
        using var allowed = new PdfDocumentFacade(allowedCore, new RecordingLogger());
        var readable = (await allowed.OpenAsync(new DocumentSource("readable.pdf", [1]))).Value!;
        allowedCore.LastDocument!.EditingAllowed = false;
        var searched = await allowed.SearchAsync(readable.SessionId, " exact ");
        Check(allowedCore.SearchQueries.SequenceEqual([" exact "]), "Exact whitespace and independent extraction gate");
        Check(searched.IsSuccess, "Extraction-allowed search must succeed even without edit permissions");
        Check(!(await facade.SearchAsync("stale", "secret")).IsSuccess, "Stale session must be refused");
    }
}
