using Pdf.Windows.Facade;

/// <summary>
/// "Add PDFs" through the facade: one file per call and one undo step per
/// file, the shape <c>pdf-ffi</c>'s <c>import_pdf</c> gives every shell.
/// </summary>
internal static class ImportParityTests
{
    private static void Check(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }

    public static async Task RunAsync()
    {
        await AppendsEveryPageOfTheImportedPdfAsync();
        await AsksForTheSourcePasswordAsync();
        await NamesTheCoreRefusalAsync();
        await RefusesARetiredSessionAsync();
        await KeepsTheImportWhenThePreviewFailsAsync();
    }

    private static async Task<(FakeCore Core, PdfDocumentFacade Facade, DocumentSession Session)> OpenAsync(FakeCore core)
    {
        var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("three.pdf", [1]))).Value!;
        core.LastDocument!.Widths(100, 101, 102);
        return (core, facade, session);
    }

    private static async Task AppendsEveryPageOfTheImportedPdfAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3, ImportWarnings = ["a form field was renamed"] });
        using var _ = facade;
        var before = (await facade.DocumentBlocksAsync(session.SessionId)).Value!.Revision;

        var result = await facade.ImportPdfAsync(session.SessionId, [7, 7], password: null, index: session.PageCount);

        Check(result.IsSuccess, "a permitted import must succeed");
        var imported = result.Value!;
        Check(imported.PageCount == 2 && imported.Session.PageCount == 5, "every source page is appended");
        Check(imported.Session.Pages.Select(page => page.WidthPt).SequenceEqual([100, 101, 102, 300, 300]),
            "existing pages keep their order and the imported ones follow");
        Check(imported.Warnings.SequenceEqual(["a form field was renamed"]), "the core's warnings reach the shell unchanged");
        Check(core.Imports.Single() == (3u, (string?)null), "the end position and the absent password reach the core");
        Check(core.RefreshPreviewCalls == 1, "the imported pages only render from a rebuilt preview");
        Check((await facade.DocumentBlocksAsync(session.SessionId)).Value!.Revision == before + 1,
            "an import is an edit: snapshots taken before it must go stale");
        var second = await facade.ImportPdfAsync(session.SessionId, [7], null, imported.Session.PageCount);
        Check(second.IsSuccess && second.Value!.SourceId != imported.SourceId, "each import names its own source");
        var blocked = await facade.OpenAsync(new DocumentSource("other.pdf", [2]));
        Check(!blocked.IsSuccess && blocked.Error!.RequiresPendingEditDecision, "imported pages are unsaved work");
    }

    private static async Task AsksForTheSourcePasswordAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3, ImportPassword = "source" });
        using var _ = facade;

        var missing = await facade.ImportPdfAsync(session.SessionId, [7], null, 3);
        var wrong = await facade.ImportPdfAsync(session.SessionId, [7], "nope", 3);

        Check(!missing.IsSuccess && missing.Error!.RequiresPassword, "a locked source asks for its password");
        Check(!wrong.IsSuccess && wrong.Error!.RequiresPassword, "a wrong password asks again");
        Check(core.RefreshPreviewCalls == 0 && core.LastDocument!.PageCount == 3, "nothing is added until the source opens");
        var unlocked = await facade.ImportPdfAsync(session.SessionId, [7], "source", 3);
        Check(unlocked.IsSuccess && unlocked.Value!.Session.PageCount == 5, "the right password imports the pages");
    }

    private static async Task NamesTheCoreRefusalAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3, ImportRefusal = "the PDF does not permit copying its pages" });
        using var _ = facade;

        var result = await facade.ImportPdfAsync(session.SessionId, [7], null, 3);

        Check(!result.IsSuccess && result.Error!.Message == "The PDF does not permit copying its pages.",
            "the core's own reason is the reader's message, not a generic \"not supported\"");
        Check(!result.Error!.RequiresPassword, "a refusal is not a password prompt");
        Check(core.RefreshPreviewCalls == 0 && core.LastDocument!.PageCount == 3, "a refused import changes nothing");
    }

    private static async Task RefusesARetiredSessionAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3 });
        using var _ = facade;

        var result = await facade.ImportPdfAsync("stale", [7], null, 3);

        Check(!result.IsSuccess && core.Imports.Count == 0, "another session's import must not reach the core");
        Check((await facade.SessionAsync(session.SessionId)).Value!.PageCount == 3, "the open document is untouched");
    }

    private static async Task KeepsTheImportWhenThePreviewFailsAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3, RefreshPreviewThrows = true });
        using var _ = facade;

        var result = await facade.ImportPdfAsync(session.SessionId, [7], null, 3);

        Check(!result.IsSuccess, "a preview that cannot be rebuilt is reported");
        Check((await facade.SessionAsync(session.SessionId)).Value!.PageCount == 5,
            "the pages are already in the document, so the session must say so: undo is how they leave");
    }
}

sealed partial class FakeDocument
{
    /// <summary>What a real import leaves behind: pages inserted, history moved on.</summary>
    public void InsertImportedPages(uint index, uint count)
    {
        _pages.InsertRange((int)index, Enumerable.Range(0, (int)count).Select(_ => new PdfCorePageDimensions(300, 842, PageRotation.None)));
        CanUndo = true;
        CanRedo = false;
    }
}
