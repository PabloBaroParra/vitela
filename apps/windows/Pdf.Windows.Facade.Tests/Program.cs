using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

var tests = new (string Name, Func<Task> Run)[]
{
    ("maps typed password failures without diagnostics", MapsTypedPasswordFailureAsync),
    ("flags password failures as recoverable, others not", FlagsPasswordFailuresAsRecoverableAsync),
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
    ("navigates to a selected search result", NavigatesToSearchResultAsync),
    ("resolves 100% zoom to 96 DPI and 4/3 DIPs per point", ResolvesHundredPercentZoom),
    ("accounts for display scale when resolving render DPI", AccountsForDisplayScaleWhenResolvingRenderDpi),
    ("fits a page to the viewport width", FitsPageToViewportWidth),
    ("fits a whole page inside the viewport", FitsWholePageInsideViewport),
    ("keeps a custom zoom independent of the viewport", KeepsCustomZoomIndependentOfViewport),
    ("clamps the zoom factor to the supported range", ClampsZoomFactor),
    ("falls back to 100% before the viewport is measured", FallsBackToHundredPercentWithoutViewport),
    ("caps render DPI with a per-page pixel ceiling", CapsRenderDpiByPixelCeiling),
    ("walks the zoom ladder and stops at both ends", WalksZoomLadder),
    ("resolves a well-formed box for a degenerate page", ResolvesDegeneratePage),
    ("requests a first render once a page has a target DPI", RequestsFirstRender),
    ("keeps a stale page bitmap when the zoom changes", KeepsStaleBitmapAcrossZoom),
    ("retargets a page render when display scale changes", RetargetsPageRenderWhenDisplayScaleChanges),
    ("does not queue a second render while one is in flight", DoesNotQueueConcurrentRenders),
    ("releases stale completed work for immediate re-request", ReleasesStaleCompletedWorkForImmediateReRequest),
    ("re-requests a render that finished at a superseded zoom", ReRequestsSupersededRender),
    ("settles a page once its render matches the current zoom", SettlesPageAtCurrentZoom),
    ("needs a render again after its bitmap is evicted", NeedsRenderAfterEviction),
    ("resolves the pages a viewport shows", ResolvesVisiblePages),
    ("keeps the page straddling the top of the viewport", KeepsPageStraddlingViewportTop),
    ("clamps an expanded window to the document", ClampsExpandedWindowToDocument),
    ("admits visible pages before prefetch after a zoom retarget", AdmitsVisiblePagesBeforePrefetchAfterZoomRetarget),
    ("restores prefetch after visible pages reach a display-scale target", RestoresPrefetchAfterDisplayScaleRetarget),
    ("drops pages the zoom left behind out of the render window", DropsPagesLeftBehindByZoom)
    ,("uses 576 DPI viewport tiles for Letter at 600% on a 100% display", UsesLetterViewportTilesAtSixHundredPercent)
    ,("uses 600 DPI viewport tiles for A4 at 600% on high-DPI displays", UsesA4ViewportTilesAtSixHundredPercentOnHighDpiDisplay)
    ,("keeps every viewport tile within the strict pixel budget", KeepsViewportTilesWithinPixelBudget)
    ,("covers the viewport without pixel rounding seams", CoversViewportWithoutPixelRoundingSeams)
    ,("bridges a tiled page's base bitmap above the zoomed-out floor", BridgesTiledPageBaseAboveTheZoomedOutFloor)
    ,("leaves an untiled page's render DPI alone", LeavesUntiledPageRenderDpiAlone)
    ,("agrees with the tile plan about when tiles are used", AgreesWithTilePlanAboutWhenTilesAreUsed)
    ,("anchors viewport tiles to a fixed page grid", AnchorsViewportTilesToAFixedPageGrid)
    ,("keeps the tile set stable while scrolling inside one tile", KeepsTileSetStableWhileScrollingInsideOneTile)
    ,("does not schedule offscreen tiles while a visible tile is outstanding", DoesNotScheduleOffscreenTilesBeforeVisibleTile)
    ,("covers a viewport of tiles in a single core call", CoversAViewportOfTilesInOneCoreCallAsync)
    ,("keeps a tile batch from cancelling the page render", KeepsTileBatchFromCancellingThePageRenderAsync)
    ,("discards a tile batch the viewport superseded", DiscardsSupersededTileBatchAsync)
    ,("discards a tile batch after the document session changes", DiscardsTileBatchAfterSessionSwapAsync)
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

static async Task FlagsPasswordFailuresAsRecoverableAsync()
{
    using var passwordFacade = new PdfDocumentFacade(new FakeCore { OpenError = PdfCoreError.PasswordRequired }, new RecordingLogger());
    var passwordResult = await passwordFacade.OpenAsync(new DocumentSource("protected.pdf", [1]));
    Assert(!passwordResult.IsSuccess, "encrypted open should fail");
    Assert(passwordResult.Error!.RequiresPassword, "a password failure must be flagged so the UI can prompt");

    using var brokenFacade = new PdfDocumentFacade(new FakeCore { OpenError = PdfCoreError.Io }, new RecordingLogger());
    var brokenResult = await brokenFacade.OpenAsync(new DocumentSource("broken.pdf", [1]));
    Assert(!brokenResult.IsSuccess, "a broken open should fail");
    Assert(!brokenResult.Error!.RequiresPassword, "a non-password failure must not be flagged as a password prompt");
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

static Task ResolvesHundredPercentZoom()
{
    var box = PageZoom.Resolve(ZoomSetting.Custom(1.0), 612, 792, new ViewportSize(1000, 800));
    AssertClose(box.WidthDips, 816, "100% should map 612pt to 816 DIPs (96/72)");
    AssertClose(box.HeightDips, 1056, "100% should map 792pt to 1056 DIPs");
    Assert(box.RenderDpi == 96, "100% should render one pixel per DIP, that is 96 DPI");
    AssertClose(box.Factor, 1.0, "a custom 100% zoom should resolve to factor 1");
    return Task.CompletedTask;
}

static Task AccountsForDisplayScaleWhenResolvingRenderDpi()
{
    var viewport = new ViewportSize(1000, 800);
    Assert(PageZoom.Resolve(ZoomSetting.Custom(1.0), 612, 792, viewport, 1.25).RenderDpi == 120, "125% display scale should render 100% zoom at 120 DPI");
    Assert(PageZoom.Resolve(ZoomSetting.Custom(1.0), 612, 792, viewport, 1.5).RenderDpi == 144, "150% display scale should render 100% zoom at 144 DPI");
    Assert(PageZoom.Resolve(ZoomSetting.Custom(1.0), 612, 792, viewport, 2.0).RenderDpi == 192, "200% display scale should render 100% zoom at 192 DPI");
    Assert(PageZoom.Resolve(ZoomSetting.Custom(1.5), 612, 792, viewport, 1.5).RenderDpi == 216, "zoom and display scale should multiply for render DPI");
    return Task.CompletedTask;
}

static Task FitsPageToViewportWidth()
{
    var box = PageZoom.Resolve(ZoomSetting.FitWidth, 612, 792, new ViewportSize(816, 400));
    AssertClose(box.WidthDips, 816, "fit-width should fill the viewport width exactly");
    AssertClose(box.Factor, 1.0, "816 DIPs across 612pt is exactly 100%");
    AssertClose(box.HeightDips, 1056, "fit-width should preserve the page aspect ratio");
    return Task.CompletedTask;
}

static Task FitsWholePageInsideViewport()
{
    var box = PageZoom.Resolve(ZoomSetting.FitPage, 612, 792, new ViewportSize(816, 792));
    AssertClose(box.HeightDips, 792, "fit-page should bind on the tighter dimension");
    Assert(box.WidthDips <= 816, "fit-page must never exceed the viewport width");
    AssertClose(box.Factor, 0.75, "792 DIPs across 792pt is 75%");
    return Task.CompletedTask;
}

/// <summary>
/// The spec's persistence criterion: a 150% zoom stays 150% while the user
/// navigates. It holds because a custom zoom is a pure function of the page,
/// never of the scroll position or the viewport.
/// </summary>
static Task KeepsCustomZoomIndependentOfViewport()
{
    var first = PageZoom.Resolve(ZoomSetting.Custom(1.5), 612, 792, new ViewportSize(1000, 800));
    var second = PageZoom.Resolve(ZoomSetting.Custom(1.5), 612, 792, new ViewportSize(300, 2000));
    Assert(first == second, "a custom zoom must not drift with the viewport, so it survives navigation and resize");
    AssertClose(first.WidthDips, 1224, "150% should map 612pt to 1224 DIPs");
    Assert(first.RenderDpi == 144, "150% should render at 144 DPI");
    return Task.CompletedTask;
}

static Task ClampsZoomFactor()
{
    Assert(ZoomSetting.Custom(50).Factor == PageZoom.MaxFactor, "an over-large zoom should clamp to the maximum");
    Assert(ZoomSetting.Custom(0.0001).Factor == PageZoom.MinFactor, "a tiny zoom should clamp to the minimum");
    var tinyPage = PageZoom.Resolve(ZoomSetting.FitWidth, 1, 1, new ViewportSize(10000, 10000));
    Assert(tinyPage.Factor <= PageZoom.MaxFactor, "a fit mode must clamp exactly like a custom zoom");
    return Task.CompletedTask;
}

static Task FallsBackToHundredPercentWithoutViewport()
{
    var box = PageZoom.Resolve(ZoomSetting.FitWidth, 612, 792, new ViewportSize(0, 0));
    AssertClose(box.Factor, 1.0, "an unmeasured viewport cannot be fitted, so fall back to 100%");
    return Task.CompletedTask;
}

/// <summary>
/// Deep zoom must degrade the bitmap, never the layout: the on-screen box
/// stays at the requested zoom while the render resolution is capped, so a
/// page at 800% cannot allocate an unbounded bitmap.
/// </summary>
static Task CapsRenderDpiByPixelCeiling()
{
    var box = PageZoom.Resolve(ZoomSetting.Custom(PageZoom.MaxFactor), 595, 842, new ViewportSize(1000, 800));
    Assert(box.RenderDpi <= PageZoom.MaxRenderDpi, "render DPI must respect the hard ceiling");
    var pixels = (long)(595 * box.RenderDpi / 72.0) * (long)(842 * box.RenderDpi / 72.0);
    Assert(pixels <= PageZoom.MaxRenderPixels, $"one page bitmap must stay under the pixel ceiling, was {pixels}");
    AssertClose(box.WidthDips, 595 * PageZoom.MaxFactor * PageZoom.DipsPerPointAt100, "capping DPI must not shrink the on-screen box");
    return Task.CompletedTask;
}

static Task WalksZoomLadder()
{
    Assert(PageZoom.StepIn(1.0).Factor == 1.25, "stepping in from 100% should reach 125%");
    Assert(PageZoom.StepOut(1.0).Factor == 0.75, "stepping out from 100% should reach 75%");
    Assert(PageZoom.StepIn(1.3).Factor == 1.5, "stepping in off the ladder should take the next rung up");
    Assert(PageZoom.StepOut(1.3).Factor == 1.25, "stepping out off the ladder should take the next rung down");
    Assert(PageZoom.StepIn(PageZoom.MaxFactor).Factor == PageZoom.MaxFactor, "stepping in at the top should stay put");
    Assert(PageZoom.StepOut(PageZoom.MinFactor).Factor == PageZoom.MinFactor, "stepping out at the bottom should stay put");
    Assert(PageZoom.StepIn(1.0).Mode == PageZoomMode.Custom, "stepping should leave the fit modes for an explicit zoom");
    return Task.CompletedTask;
}

static Task ResolvesDegeneratePage()
{
    var box = PageZoom.Resolve(ZoomSetting.FitWidth, 0, 792, new ViewportSize(816, 400));
    Assert(box.WidthDips > 0 && box.HeightDips > 0, "a page without usable dimensions still needs a well-formed placeholder");
    Assert(box.RenderDpi >= PageZoom.MinRenderDpi, "a degenerate page must still request a renderable DPI");
    return Task.CompletedTask;
}

static Task RequestsFirstRender()
{
    var plan = new PageRenderPlan();
    Assert(!plan.ShouldRequest, "a page with no target DPI has nothing to render yet");
    plan.RetargetTo(96);
    Assert(plan.ShouldRequest, "a page that has never rendered should request its first bitmap");
    Assert(!plan.HasBitmap, "nothing has been rendered yet");
    return Task.CompletedTask;
}

/// <summary>
/// The zoom responsiveness rule: changing zoom must never blank a page. The
/// bitmap already on screen stays and is scaled into the new box, so the only
/// thing the reader waits for is sharpness, not content.
/// </summary>
static Task KeepsStaleBitmapAcrossZoom()
{
    var plan = new PageRenderPlan();
    plan.RetargetTo(96);
    plan.MarkRequested();
    Assert(plan.CompleteWith(96), "the first render matches the zoom that asked for it");

    plan.RetargetTo(144);
    Assert(plan.HasBitmap, "the previous bitmap must survive a zoom change, not be cleared");
    Assert(plan.NeedsRender, "the surviving bitmap is stale and a sharper one is owed");
    Assert(plan.ShouldRequest, "the stale page should queue a render at the new zoom");
    return Task.CompletedTask;
}

static Task RetargetsPageRenderWhenDisplayScaleChanges()
{
    var plan = new PageRenderPlan();
    plan.RetargetTo(PageZoom.Resolve(ZoomSetting.Custom(1.0), 612, 792, new ViewportSize(1000, 800), 1.0).RenderDpi);
    plan.MarkRequested();
    Assert(plan.CompleteWith(96), "the initial render should settle at the initial display scale");

    plan.RetargetTo(PageZoom.Resolve(ZoomSetting.Custom(1.0), 612, 792, new ViewportSize(1000, 800), 1.5).RenderDpi);
    Assert(plan.HasBitmap, "a display-scale change must retain the current bitmap while the replacement renders");
    Assert(plan.TargetDpi == 144 && plan.ShouldRequest, "a display-scale change must request a bitmap at the new DPI");
    return Task.CompletedTask;
}

static Task DoesNotQueueConcurrentRenders()
{
    var plan = new PageRenderPlan();
    plan.RetargetTo(96);
    plan.MarkRequested();
    Assert(!plan.ShouldRequest, "a page with a render in flight must not queue a second one");
    return Task.CompletedTask;
}

/// <summary>
/// A successful facade render can be obsolete before its pixels are copied into
/// a WinUI bitmap. That work must release the request immediately so the page
/// can start the current-DPI render without waiting for materialization.
/// </summary>
static Task ReleasesStaleCompletedWorkForImmediateReRequest()
{
    var plan = new PageRenderPlan();
    plan.RetargetTo(96);
    plan.MarkRequested();
    plan.RetargetTo(288);

    Assert(plan.DiscardIfSuperseded(96), "a completed render at an old DPI must be discarded before materialization");
    Assert(plan.ShouldRequest, "discarding stale completed work must make the current target immediately requestable");
    return Task.CompletedTask;
}

/// <summary>
/// The bug this class exists to prevent: a render that lands after the zoom
/// moved is useless, and dropping it silently left the page stuck until the
/// next scroll event. It has to ask again.
/// </summary>
static Task ReRequestsSupersededRender()
{
    var plan = new PageRenderPlan();
    plan.RetargetTo(96);
    plan.MarkRequested();
    plan.RetargetTo(288);
    Assert(!plan.CompleteWith(96), "a render finished at a superseded zoom must not be published");
    Assert(plan.ShouldRequest, "a superseded render must leave the page asking again, never stranded");
    return Task.CompletedTask;
}

static Task SettlesPageAtCurrentZoom()
{
    var plan = new PageRenderPlan();
    plan.RetargetTo(200);
    plan.MarkRequested();
    Assert(plan.CompleteWith(200), "a render matching the current zoom should be published");
    Assert(plan.HasBitmap, "the page now shows a bitmap");
    Assert(!plan.NeedsRender && !plan.ShouldRequest, "a settled page must not re-render on every viewport walk");
    return Task.CompletedTask;
}

static Task NeedsRenderAfterEviction()
{
    var plan = new PageRenderPlan();
    plan.RetargetTo(96);
    plan.MarkRequested();
    plan.CompleteWith(96);
    plan.DropBitmap();
    Assert(!plan.HasBitmap, "eviction releases the bitmap");
    Assert(plan.ShouldRequest, "an evicted page must render again when it returns to the keep window");
    return Task.CompletedTask;
}

static Task ResolvesVisiblePages()
{
    var pages = Stack(10, height: 100);
    var window = PageWindow.Resolve(pages, viewportTop: 0, viewportHeight: 250);
    Assert(window.First == 0, "the first page starts at the top of the document");
    Assert(window.Last == 2, "a 250-DIP viewport reaches into the third 100-DIP page");
    Assert(window.Contains(1) && !window.Contains(3), "only pages inside the range are shown");
    return Task.CompletedTask;
}

static Task KeepsPageStraddlingViewportTop()
{
    var pages = Stack(10, height: 100);
    // Half of page 4 is above the fold; it is still on screen and must render.
    var window = PageWindow.Resolve(pages, viewportTop: 350, viewportHeight: 100);
    Assert(window.First == 3, "a page cut by the top of the viewport is still visible");
    Assert(window.Last == 4, "the viewport also reaches the page below it");
    return Task.CompletedTask;
}

static Task ClampsExpandedWindowToDocument()
{
    var pages = Stack(4, height: 100);
    var atStart = PageWindow.Resolve(pages, viewportTop: 0, viewportHeight: 100).Expand(2, pages.Count);
    Assert(atStart.First == 0, "the prefetch margin must not run off the front of the document");

    var atEnd = PageWindow.Resolve(pages, viewportTop: 336, viewportHeight: 100).Expand(2, pages.Count);
    Assert(atEnd.Last == 3, "the prefetch margin must not run off the end of the document");
    return Task.CompletedTask;
}

static Task AdmitsVisiblePagesBeforePrefetchAfterZoomRetarget()
{
    var pages = Plans(6, targetDpi: 96, renderedDpi: 96);
    Retarget(pages, 144);
    var visible = new PageWindow(2, 3);

    Assert(visible.ForRenderRequests(index => pages[index].NeedsRender, prefetchMargin: 1, pageCount: pages.Count) == visible,
        "a zoom retarget must admit only visible pages until they reach the new DPI");
    return Task.CompletedTask;
}

static Task RestoresPrefetchAfterDisplayScaleRetarget()
{
    var pages = Plans(6, targetDpi: 96, renderedDpi: 96);
    Retarget(pages, 144);
    var visible = new PageWindow(2, 3);

    Complete(pages[2]);
    Complete(pages[3]);

    Assert(visible.ForRenderRequests(index => pages[index].NeedsRender, prefetchMargin: 1, pageCount: pages.Count) == new PageWindow(1, 4),
        "once visible pages reach the display-scale target, the normal prefetch window must resume");
    return Task.CompletedTask;
}

/// <summary>
/// The bug this type exists to prevent. Zoomed out, ~17 pages are on screen and
/// all of them have a render in flight. Zooming all the way in retargets every
/// page to a far higher DPI, and each of those renders lands superseded — so
/// each one asks again, at the new DPI, for a page that is no longer on screen.
/// Seventeen multi-megapixel renders of pages nobody is looking at then starve
/// the one page that is, and the view takes seconds to sharpen.
///
/// A superseded render may only ask again if its page is still in the window.
/// </summary>
static Task DropsPagesLeftBehindByZoom()
{
    var zoomedOut = PageWindow.Resolve(Stack(40, height: 30), viewportTop: 0, viewportHeight: 700).Expand(1, 40);
    Assert(zoomedOut.Contains(15), "zoomed out, page 15 is on screen and legitimately renders");

    // Same scroll position, now zoomed in far enough that one page fills the view.
    var zoomedIn = PageWindow.Resolve(Stack(40, height: 900), viewportTop: 0, viewportHeight: 700).Expand(1, 40);
    Assert(zoomedIn.Contains(0), "the page being read stays in the window");
    Assert(!zoomedIn.Contains(15), "page 15 left the window and must not re-request at the new DPI");
    return Task.CompletedTask;
}

static Task UsesLetterViewportTilesAtSixHundredPercent()
{
    var box = PageZoom.Resolve(ZoomSetting.Custom(6.0), 612, 792, new ViewportSize(1600, 1000), 1.0);
    var plan = ViewportTilePlan.Resolve(612, 792, box, new ViewportRect(0, 0, 1200, 900), 1.0);
    Assert(plan.UsesTiles, "Letter at 600% must bypass the capped full-page raster");
    Assert(plan.Dpi == 576, "600% at 100% display scale must preserve 576 DPI tile quality");
    return Task.CompletedTask;
}

static Task UsesA4ViewportTilesAtSixHundredPercentOnHighDpiDisplay()
{
    var box = PageZoom.Resolve(ZoomSetting.Custom(6.0), 595, 842, new ViewportSize(1600, 1000), 2.0);
    var plan = ViewportTilePlan.Resolve(595, 842, box, new ViewportRect(0, 0, 900, 700), 2.0);
    Assert(plan.UsesTiles && plan.Dpi == 600, "A4 at 600% on 125%-200% displays must use the 600 DPI tile cap");
    return Task.CompletedTask;
}

static Task KeepsViewportTilesWithinPixelBudget()
{
    var box = PageZoom.Resolve(ZoomSetting.Custom(6.0), 612, 792, new ViewportSize(1600, 1000), 2.0);
    var plan = ViewportTilePlan.Resolve(612, 792, box, new ViewportRect(250, 120, 1500, 1100), 2.0);
    Assert(plan.VisibleTiles.All(tile => (long)tile.WidthPx * tile.HeightPx <= ViewportTilePlan.MaxTilePixels), "every tile must respect the strict per-tile budget");
    return Task.CompletedTask;
}

static Task CoversViewportWithoutPixelRoundingSeams()
{
    var box = PageZoom.Resolve(ZoomSetting.Custom(6.0), 595, 842, new ViewportSize(1600, 1000), 1.25);
    var plan = ViewportTilePlan.Resolve(595, 842, box, new ViewportRect(17.25, 31.5, 1301.75, 911.25), 1.25);
    Assert(plan.CoversViewportWithoutGaps, "adjacent tile edges must meet exactly after DIP-to-pixel rounding");
    return Task.CompletedTask;
}

static Task BridgesTiledPageBaseAboveTheZoomedOutFloor()
{
    // Jumping 10% -> 600% used to leave the base bitmap at the 24 DPI floor,
    // stretched 24x, until tiles arrived. The bridge render is what the reader
    // looks at in the meantime, so it must be far above that floor.
    var box = PageZoom.Resolve(ZoomSetting.Custom(6.0), 612, 792, new ViewportSize(1600, 1000), 1.0);
    var bridge = PageZoom.BridgeDpi(612, 792, box.RenderDpi);
    Assert(bridge > PageZoom.MinRenderDpi * 4, "a tiled page's base bitmap must be far sharper than the zoomed-out floor");
    Assert(bridge <= box.RenderDpi, "the bridge must never ask for more than the page's own render target");
    var pixels = (double)(612 * bridge / 72) * (792 * bridge / 72);
    Assert(pixels <= PageZoom.BridgeRenderPixels, "the bridge must stay inside its pixel budget");
    return Task.CompletedTask;
}

static Task LeavesUntiledPageRenderDpiAlone()
{
    // The bridge exists only to back tiles. At a zoom that rasters the whole
    // page properly there is nothing to bridge to, and lowering the DPI there
    // would be a straight quality regression.
    var box = PageZoom.Resolve(ZoomSetting.Custom(1.0), 612, 792, new ViewportSize(1000, 800), 1.0);
    Assert(!ViewportTilePlan.WouldUseTiles(box, 1.0), "100% on a 100% display must not need tiles");
    Assert(PageZoom.BridgeDpi(612, 792, box.RenderDpi) == box.RenderDpi, "an untiled page must keep its full render DPI");
    return Task.CompletedTask;
}

static Task AgreesWithTilePlanAboutWhenTilesAreUsed()
{
    // The layout pass and the tile planner must not disagree about this, or a
    // page gets a bridged base bitmap with no tiles to sharpen it.
    foreach (var factor in new[] { 0.10, 0.50, 1.0, 1.5, 2.0, 3.0, 6.0, 8.0 })
    {
        foreach (var scale in new[] { 1.0, 1.25, 2.0 })
        {
            var box = PageZoom.Resolve(ZoomSetting.Custom(factor), 612, 792, new ViewportSize(1600, 1000), scale);
            var plan = ViewportTilePlan.Resolve(612, 792, box, new ViewportRect(0, 0, 1200, 900), scale);
            Assert(
                ViewportTilePlan.WouldUseTiles(box, scale) == plan.UsesTiles,
                $"the tile predicate must match the plan at {factor:P0} on a {scale:P0} display");
        }
    }

    return Task.CompletedTask;
}

static Task AnchorsViewportTilesToAFixedPageGrid()
{
    var box = PageZoom.Resolve(ZoomSetting.Custom(6.0), 612, 792, new ViewportSize(1600, 1000), 1.0);
    var plan = ViewportTilePlan.Resolve(612, 792, box, new ViewportRect(700, 1300, 1200, 900), 1.0);
    Assert(
        plan.VisibleTiles.All(tile => tile.LeftPx % ViewportTilePlan.TileEdgePixels == 0 && tile.TopPx % ViewportTilePlan.TileEdgePixels == 0),
        "tile origins must sit on the page grid, not on the scroll offset");
    return Task.CompletedTask;
}

static Task KeepsTileSetStableWhileScrollingInsideOneTile()
{
    // Scrolling a few DIPs must not invalidate the tiles already on screen:
    // a moving tile origin re-renders the whole viewport on every scroll tick.
    var box = PageZoom.Resolve(ZoomSetting.Custom(6.0), 612, 792, new ViewportSize(1600, 1000), 1.0);
    var atRest = ViewportTilePlan.Resolve(612, 792, box, new ViewportRect(0, 1100, 1200, 900), 1.0);
    var nudged = ViewportTilePlan.Resolve(612, 792, box, new ViewportRect(0, 1112, 1200, 900), 1.0);
    Assert(atRest.VisibleTiles.SequenceEqual(nudged.VisibleTiles), "a scroll inside one tile row must not retarget the tile set");
    return Task.CompletedTask;
}

static Task DoesNotScheduleOffscreenTilesBeforeVisibleTile()
{
    var plan = ViewportTilePlan.ForTesting(
        [new TileRequest(0, 0, 1000, 1000, true), new TileRequest(1000, 0, 1000, 1000, false)]);
    Assert(plan.NextRequest() == new TileRequest(0, 0, 1000, 1000, true), "visible work must win over offscreen tile work");
    return Task.CompletedTask;
}

static async Task CoversAViewportOfTilesInOneCoreCallAsync()
{
    var core = new FakeCore { PageCount = 1 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var result = await facade.RenderPageTilesAsync(
        session.SessionId,
        0,
        576,
        [new PageRegion(0, 0, 1024, 1024), new PageRegion(1024, 0, 1024, 1024), new PageRegion(0, 1024, 1024, 1024)],
        false);

    Assert(result.IsSuccess && result.Value!.Count == 3, "a tile batch must return one bitmap per requested tile");
    Assert(core.TileBatchSizes.Count == 1, "covering a viewport must cost one core call, not one per tile");
}

static async Task KeepsTileBatchFromCancellingThePageRenderAsync()
{
    // The two describe different things about the same page. Sharing one
    // pending slot would let a deep-zoom tile batch cancel the base render the
    // page still needs, and vice versa.
    var core = new FakeCore { PageCount = 1 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var page = facade.RenderPageAsync(session.SessionId, 0, 96, false);
    var tiles = facade.RenderPageTilesAsync(session.SessionId, 0, 576, [new PageRegion(0, 0, 1024, 1024)], false);

    Assert((await page).IsSuccess, "a tile batch must not discard the page's own render");
    Assert((await tiles).IsSuccess, "a page render must not discard the tile batch");
}

static async Task DiscardsSupersededTileBatchAsync()
{
    var core = new FakeCore { PageCount = 1, BlockFirstRender = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var first = facade.RenderPageTilesAsync(session.SessionId, 0, 576, [new PageRegion(0, 0, 1024, 1024)], false);
    await core.FirstRenderStarted.Task;
    var second = facade.RenderPageTilesAsync(session.SessionId, 0, 576, [new PageRegion(1024, 0, 1024, 1024)], false);
    core.ReleaseFirstRender.Set();

    Assert((await first.WaitAsync(TimeSpan.FromSeconds(5))).IsDiscarded, "a tile batch the viewport moved past must not publish");
    Assert((await second.WaitAsync(TimeSpan.FromSeconds(5))).IsSuccess, "the newest tile batch must still be served");
}

static async Task DiscardsTileBatchAfterSessionSwapAsync()
{
    var core = new FakeCore { PageCount = 1, BlockFirstRender = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;

    var batch = facade.RenderPageTilesAsync(session.SessionId, 0, 576, [new PageRegion(0, 0, 1024, 1024)], false);
    await core.FirstRenderStarted.Task;
    await facade.OpenAsync(new DocumentSource("second.pdf", [2]));
    core.ReleaseFirstRender.Set();

    Assert((await batch.WaitAsync(TimeSpan.FromSeconds(5))).IsDiscarded, "a tile batch from a retired document session must not publish");
}

static List<PageSpan> Stack(int count, double height)
{
    var pages = new List<PageSpan>(count);
    for (var index = 0; index < count; index++)
    {
        pages.Add(new PageSpan(index * (height + 12), height));
    }

    return pages;
}

static List<PageRenderPlan> Plans(int count, uint targetDpi, uint renderedDpi)
{
    var plans = new List<PageRenderPlan>(count);
    for (var index = 0; index < count; index++)
    {
        var plan = new PageRenderPlan();
        plan.RetargetTo(renderedDpi);
        plan.MarkRequested();
        Assert(plan.CompleteWith(renderedDpi), "the initial page bitmap should settle");
        plan.RetargetTo(targetDpi);
        plans.Add(plan);
    }

    return plans;
}

static void Retarget(IEnumerable<PageRenderPlan> plans, uint dpi)
{
    foreach (var plan in plans)
    {
        plan.RetargetTo(dpi);
    }
}

static void Complete(PageRenderPlan plan)
{
    plan.MarkRequested();
    Assert(plan.CompleteWith(plan.TargetDpi), "the current-DPI render should settle");
}

static void Assert(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}

static void AssertClose(double actual, double expected, string message)
{
    if (Math.Abs(actual - expected) > 1e-6)
    {
        throw new InvalidOperationException($"{message} (expected {expected}, got {actual})");
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

    /// <summary>How many tiles each batch asked for — one entry per core call.</summary>
    public System.Collections.Concurrent.ConcurrentQueue<int> TileBatchSizes { get; } = new();

    public IReadOnlyList<PdfCoreBitmap> RenderPageTiles(IPdfCoreDocument document, uint pageIndex, uint dpi, IReadOnlyList<PageRegion> tiles, bool invertContentColors)
    {
        TileBatchSizes.Enqueue(tiles.Count);
        return [.. tiles.Select(_ => RenderPage(document, pageIndex, dpi, invertContentColors))];
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
