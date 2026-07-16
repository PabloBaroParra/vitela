using Pdf.Windows.Facade;

var tests = new (string Name, Func<Task> Run)[]
{
    ("maps typed password failures without diagnostics", MapsTypedPasswordFailureAsync),
    ("maps selected-file read failures to user-safe results", MapsReadFailureAsync),
    ("navigates after a page render failure", NavigatesAfterRenderFailureAsync),
    ("discards stale page render results", DiscardsStaleRenderResultAsync),
    ("maps blank render document-not-found to empty", MapsBlankRenderToEmptyAsync),
    ("completes superseded renders after a session swap", CompletesRendersAfterSessionSwapAsync),
    ("defers document disposal until in-flight render completes", DefersDisposalUntilRenderCompletesAsync),
    ("packs padded renderer rows tightly", PacksPaddedRowsTightlyAsync),
    ("exposes page dimensions on the session", ExposesPageDimensionsAsync),
    ("renders any page independently of the current page index", RendersPagesIndependentlyOfCurrentIndexAsync),
    ("renders a print page independently of viewer renders", RendersPrintPageIndependentlyAsync),
    ("discards a print page after a session swap", DiscardsPrintPageAfterSessionSwapAsync),
    ("discards stale search results", DiscardsStaleSearchResultAsync),
    ("navigates to a selected search result", NavigatesToSearchResultAsync)
};

foreach (var test in tests)
{
    await test.Run();
    Console.WriteLine($"PASS {test.Name}");
}

static async Task MapsTypedPasswordFailureAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore { OpenError = PdfCoreError.WrongPassword }, new RecordingLogger());
    var result = await facade.OpenAsync(new DocumentSource("protected.pdf", [1]));
    Assert(!result.IsSuccess, "open should fail");
    Assert(result.Error!.Message == "This document requires a password.", "password error should be user safe");
    Assert(!result.Error.Message.Contains("WrongPassword", StringComparison.Ordinal), "error should not expose diagnostics");
}

static Task MapsReadFailureAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
    var result = facade.OpenReadFailure(new IOException("sensitive path"));
    Assert(!result.IsSuccess, "read failure should fail");
    Assert(result.Error!.Message == "The document could not be processed.", "read failure should be user safe");
    return Task.CompletedTask;
}

static async Task NavigatesAfterRenderFailureAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore { PageCount = 2, RenderError = PdfCoreError.RenderFailed }, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var render = await facade.RenderCurrentPageAsync(session.SessionId, 144, false);
    var navigation = await facade.NavigateAsync(session.SessionId, 1);
    Assert(!render.IsSuccess, "initial render should fail");
    Assert(navigation.Value!.PageIndex == 1, "navigation should remain available");
}

static async Task DiscardsStaleRenderResultAsync()
{
    var core = new FakeCore { PageCount = 1, BlockFirstRender = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var first = facade.RenderCurrentPageAsync(session.SessionId, 144, false);
    await core.FirstRenderStarted.Task;
    var second = facade.RenderCurrentPageAsync(session.SessionId, 144, false);
    core.ReleaseFirstRender.Set();
    var firstResult = await first;
    var secondResult = await second;
    Assert(firstResult.IsDiscarded, "superseded render should be discarded");
    Assert(secondResult.IsSuccess, "latest render should succeed");
    Assert(secondResult.Value!.Sequence == 2, "latest render should retain the newest sequence");
}

static async Task MapsBlankRenderToEmptyAsync()
{
    var core = new FakeCore { PageCount = 0, RenderError = PdfCoreError.DocumentNotFound };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.CreateBlankAsync()).Value!;
    var result = await facade.RenderCurrentPageAsync(session.SessionId, 144, false);
    Assert(result.IsEmpty, "blank document render should be an empty state");
}

static async Task CompletesRendersAfterSessionSwapAsync()
{
    var core = new FakeCore { PageCount = 1, BlockFirstRender = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    var first = facade.RenderCurrentPageAsync(session.SessionId, 144, false);
    await core.FirstRenderStarted.Task;
    var second = facade.RenderCurrentPageAsync(session.SessionId, 144, false);
    var swapped = await facade.OpenAsync(new DocumentSource("second.pdf", [2]));
    Assert(swapped.IsSuccess, "session swap should succeed");
    core.ReleaseFirstRender.Set();
    var completed = await Task.WhenAll(first, second).WaitAsync(TimeSpan.FromSeconds(5));
    Assert(completed[0].IsDiscarded, "in-flight render for a replaced session should be discarded");
    Assert(completed[1].IsDiscarded, "queued render for a replaced session should be discarded, not left hanging");
}

static async Task DefersDisposalUntilRenderCompletesAsync()
{
    var core = new FakeCore { PageCount = 1, BlockFirstRender = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    var firstDocument = core.LastDocument!;
    var first = facade.RenderCurrentPageAsync(session.SessionId, 144, false);
    await core.FirstRenderStarted.Task;
    await facade.OpenAsync(new DocumentSource("second.pdf", [2]));
    Assert(!firstDocument.Disposed, "a document with an in-flight render must not be disposed");
    core.ReleaseFirstRender.Set();
    await first.WaitAsync(TimeSpan.FromSeconds(5));
    Assert(firstDocument.Disposed, "a replaced document should be disposed once its render completes");
}

static Task PacksPaddedRowsTightlyAsync()
{
    byte[] padded = [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 9, 10, 11, 12, 13, 14, 15, 16, 0, 0, 0, 0];
    var tight = PdfBitmapRows.TightlyPacked(padded, 2, 2, 12);
    Assert(tight.SequenceEqual((byte[])[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]), "padded rows should repack to width * 4");
    byte[] alreadyTight = [1, 2, 3, 4];
    Assert(ReferenceEquals(PdfBitmapRows.TightlyPacked(alreadyTight, 1, 1, 4), alreadyTight), "tight buffers should pass through unchanged");
    return Task.CompletedTask;
}

static async Task ExposesPageDimensionsAsync()
{
    var core = new FakeCore { PageCount = 2, PageWidthPt = 595, PageHeightPt = 842 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    Assert(session.Pages.Count == 2, "session should carry one dimension entry per page");
    Assert(session.Pages[1].WidthPt == 595 && session.Pages[1].HeightPt == 842, "dimensions should pass through in points");
}

static async Task RendersPagesIndependentlyOfCurrentIndexAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore { PageCount = 3 }, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var result = await facade.RenderPageAsync(session.SessionId, 2, 144, false);
    Assert(result.IsSuccess, "an off-current page render should succeed, not be discarded");
    Assert(result.Value!.PageIndex == 2, "the rendered page should carry its own index");
}

static async Task RendersPrintPageIndependentlyAsync()
{
    var core = new FakeCore { PageCount = 2, BlockFirstRender = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var viewerRender = facade.RenderPageAsync(session.SessionId, 0, 144, false);
    await core.FirstRenderStarted.Task;
    var print = facade.RenderPageForPrintAsync(session.SessionId, 1, 300, false);
    core.ReleaseFirstRender.Set();
    var printResult = await print;
    Assert(printResult.IsSuccess, "print render should not be superseded by an in-flight viewer render");
    Assert(printResult.Value!.PageIndex == 1, "print render should carry its own page index");
    Assert(core.RenderDpis.Any(dpi => dpi == 300), "print render should use print DPI");
    await viewerRender;
}

static async Task DiscardsPrintPageAfterSessionSwapAsync()
{
    var core = new FakeCore { PageCount = 2, BlockFirstRender = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    var print = facade.RenderPageForPrintAsync(session.SessionId, 0, 300, false);
    await core.FirstRenderStarted.Task;
    await facade.OpenAsync(new DocumentSource("second.pdf", [2]));
    core.ReleaseFirstRender.Set();
    var result = await print.WaitAsync(TimeSpan.FromSeconds(5));
    Assert(result.IsDiscarded, "a print render for a replaced session must be discarded");
}

static async Task DiscardsStaleSearchResultAsync()
{
    var core = new FakeCore { BlockFirstSearch = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var first = facade.SearchAsync(session.SessionId, "first");
    await core.FirstSearchStarted.Task;
    var second = facade.SearchAsync(session.SessionId, "second");
    core.ReleaseFirstSearch.Set();
    var firstResult = await first;
    var secondResult = await second;
    Assert(firstResult.IsDiscarded, "superseded search should be discarded");
    Assert(secondResult.IsSuccess && secondResult.Value!.Query == "second", "only the latest query may publish results");
}

static async Task NavigatesToSearchResultAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore { PageCount = 3 }, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var result = await facade.NavigateToSearchResultAsync(session.SessionId, new SearchHit(2, "match", []));
    Assert(result.IsSuccess && result.Value!.PageIndex == 2, "selected search result should navigate to its page");
}

static void Assert(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}

sealed class FakeCore : IPdfCore
{
    public uint PageCount { get; init; } = 1;
    public double PageWidthPt { get; init; } = 595;
    public double PageHeightPt { get; init; } = 842;
    public PdfCoreError? OpenError { get; init; }
    public PdfCoreError? RenderError { get; init; }
    public bool BlockFirstRender { get; init; }
    public bool BlockFirstSearch { get; init; }
    public TaskCompletionSource FirstRenderStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
    public ManualResetEventSlim ReleaseFirstRender { get; } = new(false);
    public TaskCompletionSource FirstSearchStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
    public ManualResetEventSlim ReleaseFirstSearch { get; } = new(false);
    public System.Collections.Concurrent.ConcurrentQueue<uint> RenderDpis { get; } = new();
    public FakeDocument? LastDocument { get; private set; }
    private int _renderCount;
    private int _searchCount;

    public IPdfCoreDocument OpenFromBytes(byte[] bytes, string? password)
    {
        if (OpenError is { } error)
        {
            throw new PdfCoreException(error, "sensitive diagnostic");
        }

        return LastDocument = new FakeDocument(PageCount, PageWidthPt, PageHeightPt);
    }

    public IPdfCoreDocument CreateBlank() => LastDocument = new FakeDocument(PageCount, PageWidthPt, PageHeightPt);

    public PdfCoreBitmap RenderPage(IPdfCoreDocument document, uint pageIndex, uint dpi, bool invertContentColors)
    {
        RenderDpis.Enqueue(dpi);
        if (Interlocked.Increment(ref _renderCount) == 1 && BlockFirstRender)
        {
            FirstRenderStarted.SetResult();
            ReleaseFirstRender.Wait(TimeSpan.FromSeconds(5));
        }

        if (RenderError is { } error)
        {
            throw new PdfCoreException(error, "sensitive diagnostic");
        }

        return new PdfCoreBitmap(1, 1, 4, [0, 0, 0, 255]);
    }

    public IReadOnlyList<PdfCoreSearchHit> Search(IPdfCoreDocument document, string query)
    {
        if (Interlocked.Increment(ref _searchCount) == 1 && BlockFirstSearch)
        {
            FirstSearchStarted.SetResult();
            ReleaseFirstSearch.Wait(TimeSpan.FromSeconds(5));
        }

        return [new PdfCoreSearchHit(0, query, [new PdfCoreSearchRect(100, 700, 24, 24)])];
    }
}

sealed class FakeDocument(uint pageCount, double widthPt = 595, double heightPt = 842) : IPdfCoreDocument
{
    public uint PageCount { get; } = pageCount;
    public IReadOnlyList<PdfCorePageDimensions> PageDimensions { get; } =
        [.. Enumerable.Range(0, (int)pageCount).Select(_ => new PdfCorePageDimensions(widthPt, heightPt))];
    public bool Disposed { get; private set; }
    public void Dispose() => Disposed = true;
}

sealed class RecordingLogger : IDiagnosticLogger
{
    public void Failure(PdfCoreError category, string operation, string correlationId, string? sessionId, uint? pageIndex, string sanitizedDetail) { }
}
