using Pdf.Windows.Facade;

/// <summary>
/// "Add PDFs" through the facade, in the Linux shell's two phases: every
/// picked file is prepared on its own (progress, its own password, a cancel
/// that just drops it), then the whole pick lands as ONE undo step.
/// </summary>
internal static class ImportParityTests
{
    private static void Check(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }

    public static async Task RunAsync()
    {
        await PreparingChangesNothingAsync();
        await ImportsAWholePickAsOneEditAsync();
        await AsksForTheSourcePasswordWhilePreparingAsync();
        await NamesTheCoreRefusalAsync();
        await LeavesEverythingAsItWasWhenRefusedAsync();
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

    private static async Task<IImportSource> PrepareAsync(PdfDocumentFacade facade, byte pages, string? password = null) =>
        (await facade.PrepareImportAsync([pages], password)).Value!;

    private static async Task PreparingChangesNothingAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3, ImportWarnings = ["a form field was renamed"] });
        using var _ = facade;

        using var source = await PrepareAsync(facade, 2);

        Check(source.PageCount == 2 && source.Warnings.SequenceEqual(["a form field was renamed"]),
            "a prepared source reports its pages and warnings before anything is added");
        Check(core.LastDocument!.PageCount == 3 && core.RefreshPreviewCalls == 0 && !core.LastDocument.CanUndo,
            "preparing must not touch the open document or its history");
        Check((await facade.OpenAsync(new DocumentSource("other.pdf", [2]))).IsSuccess,
            "a prepared but unimported source is not unsaved work");
    }

    private static async Task ImportsAWholePickAsOneEditAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3 });
        using var _ = facade;
        var before = (await facade.DocumentBlocksAsync(session.SessionId)).Value!.Revision;

        var result = await facade.ImportPreparedAsync(session.SessionId, [await PrepareAsync(facade, 2), await PrepareAsync(facade, 1)], 3);

        Check(result.IsSuccess, "a permitted pick must import");
        var imported = result.Value!;
        Check(imported.PageCount == 3 && imported.Session.PageCount == 6, "every page of every source is appended");
        Check(imported.Session.Pages.Select(page => page.WidthPt).SequenceEqual([100, 101, 102, 300, 300, 301]),
            "the sources land in pick order after the existing pages");
        Check(imported.SourceIds.Count == 2 && imported.SourceIds[0] != imported.SourceIds[1], "each source names its own block");
        Check(core.BatchImports.Single() == (3u, 2), "the whole pick reaches the core as ONE import");
        Check(core.RefreshPreviewCalls == 1, "one rebuild for the whole pick");
        Check((await facade.DocumentBlocksAsync(session.SessionId)).Value!.Revision == before + 1,
            "one edit, so exactly one revision");
        var blocked = await facade.OpenAsync(new DocumentSource("other.pdf", [2]));
        Check(!blocked.IsSuccess && blocked.Error!.RequiresPendingEditDecision, "imported pages are unsaved work");
    }

    private static async Task AsksForTheSourcePasswordWhilePreparingAsync()
    {
        var (core, facade, _) = await OpenAsync(new FakeCore { PageCount = 3, ImportPassword = "source" });
        using var __ = facade;

        var missing = await facade.PrepareImportAsync([1], null);
        var wrong = await facade.PrepareImportAsync([1], "nope");
        var unlocked = await facade.PrepareImportAsync([1], "source");

        Check(!missing.IsSuccess && missing.Error!.RequiresPassword, "a locked source asks for its password");
        Check(!wrong.IsSuccess && wrong.Error!.RequiresPassword, "a wrong password asks again");
        Check(unlocked.IsSuccess, "the right password prepares the source");
        Check(core.LastDocument!.PageCount == 3, "asking for a password never touches the document");
    }

    private static async Task NamesTheCoreRefusalAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3, ImportRefusal = "this document does not permit changing its content" });
        using var _ = facade;

        var asked = await facade.ImportRefusalAsync(session.SessionId);
        var result = await facade.ImportPreparedAsync(session.SessionId, [await PrepareAsync(facade, 1)], 3);

        Check(asked.IsSuccess && asked.Value == "This document does not permit changing its content.",
            "the refusal can be asked before the picker, in the core's own words");
        Check(!result.IsSuccess && result.Error!.Message == "This document does not permit changing its content.",
            "the import is refused with the same reason, not a generic \"not supported\"");
        Check(core.RefreshPreviewCalls == 0 && core.LastDocument!.PageCount == 3, "a refused import changes nothing");
    }

    private static async Task LeavesEverythingAsItWasWhenRefusedAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3 });
        using var _ = facade;
        Check((await facade.ImportRefusalAsync(session.SessionId)).Value is null, "an editable document has no refusal");
        var source = await PrepareAsync(facade, 1);

        var refused = await facade.ImportPreparedAsync(session.SessionId, [source], 9);
        var retried = await facade.ImportPreparedAsync(session.SessionId, [source], 3);
        var spent = await facade.ImportPreparedAsync(session.SessionId, [source], 4);

        Check(!refused.IsSuccess && core.RefreshPreviewCalls == 1 && retried.IsSuccess,
            "a refused pick keeps its sources, so the same pick can be imported afterwards");
        Check(!spent.IsSuccess && core.LastDocument!.PageCount == 4, "a source imports only once");
    }

    private static async Task RefusesARetiredSessionAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3 });
        using var _ = facade;

        var result = await facade.ImportPreparedAsync("stale", [await PrepareAsync(facade, 1)], 3);

        Check(!result.IsSuccess && core.BatchImports.Count == 0, "another session's import must not reach the core");
        Check(!(await facade.ImportRefusalAsync("stale")).IsSuccess, "nor may it be asked about");
        Check((await facade.SessionAsync(session.SessionId)).Value!.PageCount == 3, "the open document is untouched");
    }

    private static async Task KeepsTheImportWhenThePreviewFailsAsync()
    {
        var (_, facade, session) = await OpenAsync(new FakeCore { PageCount = 3, RefreshPreviewThrows = true });
        using var __ = facade;

        var result = await facade.ImportPreparedAsync(session.SessionId, [await PrepareAsync(facade, 2)], 3);

        Check(!result.IsSuccess, "a preview that cannot be rebuilt is reported");
        Check((await facade.SessionAsync(session.SessionId)).Value!.PageCount == 5,
            "the pages are already in the document, so the session must say so: undo is how they leave");
    }
}

/// <summary>A prepared source whose page count is the first byte it was prepared from.</summary>
sealed class FakeImportSource(uint pageCount, IReadOnlyList<string> warnings) : IImportSource
{
    public uint PageCount { get; } = pageCount;
    public IReadOnlyList<string> Warnings { get; } = warnings;
    public bool Spent { get; set; }
    public bool Disposed { get; private set; }
    public void Dispose() => Disposed = true;
}

sealed partial class FakeDocument
{
    /// <summary>What a real import leaves behind: one source's pages, each 300pt plus the source's position.</summary>
    public void InsertImportedPages(uint index, IReadOnlyList<uint> counts)
    {
        var pages = counts.SelectMany((count, source) => Enumerable.Range(0, (int)count)
            .Select(_ => new PdfCorePageDimensions(300 + source, 842, PageRotation.None)));
        _pages.InsertRange((int)index, pages);
        CanUndo = true;
        CanRedo = false;
    }
}
