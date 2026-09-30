using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

var tests = new (string Name, Func<Task> Run)[]
{
    ("replaces resource and inline image sources with history", ReplacesContentImagesAsync),
    ("refuses stale forbidden and unrecoverable image replacement", RefusesImageReplacementAsync),
    ("retains image replacement after preview failure", KeepsImageReplacementAfterPreviewFailureAsync),
    ("inserts image content with shared placement and history", InsertsContentImageAsync),
    ("refuses invalid stale and forbidden image insertion", RefusesInvalidImageInsertionAsync),
    ("keeps image insertion undoable after preview failure", KeepsImageInsertionAfterPreviewFailureAsync),
    ("deletes text without font substitution through preview and history", DeletesContentTextAsync),
    ("moves text preserving its source and shared history", MovesContentTextAsync),
    ("rejects invalid and stale text movement", RefusesInvalidTextMovementAsync),
    ("keeps text movement undoable after preview failure", KeepsTextMovementAfterPreviewFailureAsync),
    ("rejects stale and forbidden text deletion", RefusesInvalidTextDeletionAsync),
    ("keeps text deletion undoable after preview failure", KeepsTextDeletionAfterPreviewFailureAsync),
    ("deletes resource and inline images through preview and history", DeletesContentImagesAsync),
    ("rejects stale and forbidden image deletion", RefusesInvalidImageDeletionAsync),
    ("keeps a deleted image undoable after preview failure", KeepsImageDeletionAfterPreviewFailureAsync),
    ("moves resource and inline images without resizing", MovesContentImagesAsync),
    ("rejects invalid, stale and forbidden image movement", RefusesInvalidImageMovementAsync),
    ("keeps a moved image undoable after preview failure", KeepsImageMoveAfterPreviewFailureAsync),
    ("resizes resource and inline images through preview and history", ResizesContentImagesAsync),
    ("rejects invalid, stale and forbidden image resizing", RefusesInvalidImageResizingAsync),
    ("keeps a resized image undoable after preview failure", KeepsImageResizeAfterPreviewFailureAsync),
    ("maps typed password failures without diagnostics", MapsTypedPasswordFailureAsync),
    ("flags password failures as recoverable, others not", FlagsPasswordFailuresAsRecoverableAsync),
    ("maps selected-file read failures to user-safe results", MapsReadFailureAsync),
    ("navigates after a page render failure", NavigatesAfterRenderFailureAsync),
    ("discards stale page render results", DiscardsStaleRenderResultAsync),
    ("maps blank render document-not-found to empty", MapsBlankRenderToEmptyAsync),
    ("creates a document with a page to work on", CreatesADocumentWithAPageToWorkOnAsync),
    ("completes superseded renders after a session swap", CompletesRendersAfterSessionSwapAsync),
    ("defers document disposal until in-flight render completes", DefersDisposalUntilRenderCompletesAsync),
    ("packs padded renderer rows tightly", PacksPaddedRowsTightlyAsync),
    ("exposes page dimensions on the session", ExposesPageDimensionsAsync),
    ("carries each page's rotation on the session", CarriesPageRotationOnTheSessionAsync),
    ("asks the core where page geometry lands on a drawn page", AsksTheCoreWherePageGeometryLandsAsync),
    ("renders any page independently of the current page index", RendersPagesIndependentlyOfCurrentIndexAsync),
    ("renders a print page independently of viewer renders", RendersPrintPageIndependentlyAsync),
    ("renders every export page at the requested DPI without navigating", RendersExportPagesWithoutNavigatingAsync),
    ("discards a print page after a session swap", DiscardsPrintPageAfterSessionSwapAsync),
    ("discards stale search results", DiscardsStaleSearchResultAsync),
    ("navigates to a selected search result", NavigatesToSearchResultAsync),
    ("steps through search hits and wraps at either end", StepsThroughSearchHits),
    ("reports the selected search hit with the submitted query", ReportsSelectedSearchHit),
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
    ,("records annotation edits in core history", RecordsAnnotationEditsInCoreHistoryAsync)
    ,("publishes restyled annotation colors", PublishesRestyledAnnotationColorAsync)
    ,("steps backward through annotations without changing the document", StepsBackwardThroughAnnotations)
    ,("refuses annotation edits when permissions deny them", RefusesForbiddenAnnotationEditsAsync)
    ,("holds annotation edits until destination replacement completes", HoldsEditsUntilDestinationReplacementCompletesAsync)
    ,("blocks opening another document with unsaved annotations", BlocksOpenWithUnsavedAnnotationsAsync)
    ,("releases the guard once the edits are undone", ReleasesTheGuardOnceEditsAreUndoneAsync)
    ,("releases the guard once the edits are saved", ReleasesTheGuardOnceEditsAreSavedAsync)
    ,("flags the refused open as a decision the reader can make", FlagsPendingEditDecisionAsync)
    ,("opens over pending edits once the reader chose to discard them", DiscardsPendingEditsOnRequestAsync)
    ,("asks about pending edits before the password, then unlocks over a discard", UnlocksOverDiscardedPendingEditsAsync)
    ,("blocks creating a blank document with unsaved annotations", BlocksCreateBlankWithUnsavedAnnotationsAsync)
    ,("creates a blank document over pending edits once the reader chose to discard them", CreatesBlankDocumentOverPendingEditsOnRequestAsync)
    ,("keeps stamp previews scoped to their document session", KeepsStampPreviewsScopedToSession)
    ,("reconciles one inserted stamp from an annotation snapshot", ReconcilesInsertedStamp)
    ,("rejects stale stamp input sessions", RejectsStaleStampInputSession)
    ,("routes PNG and JPEG image signatures", RoutesSupportedStampSignatures)
    ,("rejects non-image files before stamp insertion", RejectsUnsupportedStampInput)
    ,("reports the core image validation failure clearly", ReportsInvalidStampImageAsync)
    ,("asks the core where a dropped stamp goes instead of sizing it", AsksTheCoreWhereAStampGoesAsync)
    ,("reports an image that cannot be measured for placement", ReportsAnUnplaceableStampImage)
    ,("routes a dropped PDF to open and a dropped image to stamp", RoutesDroppedFilesByKind)
    ,("acts on one file out of a multi-file drop", ActsOnOneDroppedFile)
    ,("writes a failure where its correlation id can be looked up", WritesRetrievableDiagnosticsAsync)
    ,("caps the diagnostic log and keeps one generation back", CapsTheDiagnosticLog)
    ,("never lets diagnostics throw into the operation", SwallowsDiagnosticWriteFailures)
    ,("maps unexpected save failures to user-safe results", MapsUnexpectedSaveFailureAsync)
    ,("refuses to silently break a signature", RefusesToSilentlyBreakASignatureAsync)
    ,("reports whether saving breaks a signature", ReportsWhetherSavingBreaksASignatureAsync)
    ,("saves a signed document once acknowledged", SavesASignedDocumentOnceAcknowledgedAsync)
    ,("protects with two distinct password roles", ProtectsWithTwoDistinctPasswordRolesAsync)
    ,("reopens protected bytes with both password roles", ReopensProtectedBytesWithBothPasswordRolesAsync)
    ,("refuses protection when content changes are forbidden", RefusesForbiddenProtectionAsync)
    ,("compresses with the chosen preset and reports both sizes", CompressesWithTheChosenPresetAsync)
    ,("refuses to silently break a signature when compressing", RefusesToSilentlyBreakASignatureWhenCompressingAsync)
    ,("compresses a signed document once acknowledged", CompressesASignedDocumentOnceAcknowledgedAsync)
    ,("reports why a document cannot be compressed", ReportsWhyADocumentCannotBeCompressedAsync)
    ,("sizes compression results in powers of ten", SizesCompressionResultsInPowersOfTen)
    ,("reports a compression's saving without rounding it up", ReportsACompressionSavingWithoutRoundingUp)
    ,("says a compression with nothing to gain wrote nothing", SaysANoGainCompressionWroteNothing)
    ,("plans an image export of every page under the core's file names", PlansAnImageExportOfEveryPageAsync)
    ,("plans an image export of the current page only", PlansAnImageExportOfTheCurrentPageAsync)
    ,("keeps a stale current page inside the document", KeepsAStaleCurrentPageInsideTheDocumentAsync)
    ,("reads a custom page range with the core's grammar", ReadsACustomPageRangeWithTheCoresGrammarAsync)
    ,("shows the core's own sentence for a bad page range", ShowsTheCoresSentenceForABadPageRangeAsync)
    ,("refuses an image export the document forbids", RefusesAForbiddenImageExportAsync)
    ,("refuses a resolution outside what the dialog offers", RefusesAResolutionOutsideTheDialogAsync)
    ,("names a page too large to export before writing anything", NamesAnOversizedPageBeforeWritingAsync)
    ,("exports a page's encoded bytes in the chosen format", ExportsAPagesEncodedBytesAsync)
    ,("refuses a page image once the session is retired", RefusesAPageImageAfterSessionSwapAsync)
    ,("summarises an image export in the singular and plural", SummarisesAnImageExport)
    ,("plans an extraction with the core's parsed pages", PlansAnExtractionWithTheCoresParsedPagesAsync)
    ,("refuses an empty page range before the core's grammar runs", RefusesAnEmptyExtractRangeAsync)
    ,("shows the core's own sentence for a bad extract range", ShowsTheCoresSentenceForABadExtractRangeAsync)
    ,("refuses an extraction the document forbids copying from", RefusesAnExtractionForbiddenByCopyingAsync)
    ,("refuses an extraction that could not be fully rewritten", RefusesAnExtractionThatCannotBeRewrittenAsync)
    ,("extracts pruned bytes for exactly the planned pages", ExtractsPrunedBytesForThePlannedPagesAsync)
    ,("refuses an extraction once the session is retired", RefusesAnExtractionAfterSessionSwapAsync)
    ,("summarises an extraction in the singular and plural", SummarisesAnExtraction)
    ,("adds a signature note only when the source is signed", AddsASignatureNoteOnlyWhenTheSourceIsSigned)
    ,("plans split files with core boundaries and source signature", PlansSplitPartsAsync)
    ,("refuses a split when copying or rewriting is forbidden", RefusesForbiddenSplitAsync)
    ,("refuses a split once the session is retired", RefusesSplitAfterSessionSwapAsync)
    ,("loads a page's characters for caret and selection queries", LoadsPageCharactersAsync)
    ,("refuses page characters once the session is retired", RefusesPageCharactersAfterSessionSwapAsync)
    ,("reads a page's editable text runs", ReadsPageContentForEditingAsync)
    ,("refuses page content when the document forbids content changes", RefusesPageContentWhenTheDocumentForbidsItAsync)
    ,("replaces a text run and refreshes the preview", ReplacesATextRunAndRefreshesThePreviewAsync)
    ,("substitutes a composite text run explicitly", SubstitutesACompositeTextRunExplicitlyAsync)
    ,("names the character a font cannot show", NamesTheCharacterAFontCannotShowAsync)
    ,("keeps a content edit when the preview refresh fails", KeepsTheEditWhenThePreviewRefreshFailsAsync)
    ,("refreshes the preview on history only after a content edit", RefreshesThePreviewOnHistoryOnlyAfterAContentEditAsync)
    ,("reads the effective document properties", ReadsEffectiveDocumentPropertiesAsync)
    ,("updates document properties without dropping dates", UpdatesDocumentPropertiesWithoutDroppingDatesAsync)
    ,("does not record an unchanged document properties edit", DoesNotRecordUnchangedDocumentPropertiesAsync)
    ,("refuses document properties when content editing is forbidden", RefusesForbiddenDocumentPropertiesAsync)
    ,("lists the document's form fields with the fill permission", ListsFormFieldsAsync)
    ,("fills a form field and rebuilds the preview", FillsAFormFieldAsync)
    ,("places a text field and refreshes the form preview", PlacesATextFieldAsync)
    ,("requires both permissions to place a text field", RefusesForbiddenTextFieldPlacementAsync)
    ,("places a checkbox and refreshes the form preview", PlacesACheckboxAsync)
    ,("requires both permissions to place a checkbox", RefusesForbiddenCheckboxPlacementAsync)
    ,("places a radio group with the shared default options", PlacesARadioGroupAsync)
    ,("requires both permissions to place a radio group", RefusesForbiddenRadioGroupPlacementAsync)
    ,("places a dropdown and refreshes the form preview", PlacesADropdownAsync)
    ,("requires both permissions to place a dropdown", RefusesForbiddenDropdownPlacementAsync)
    ,("places a form field by tracing in either direction", TracesFormPlacementInEitherDirection)
    ,("uses click dimensions for a tiny form gesture", UsesClickDimensionsForTinyFormGesture)
    ,("clamps form placement to unrotated rotated-page bounds", ClampsFormPlacementOnRotatedPages)
    ,("does not record a fill that changes nothing", DoesNotRecordAnUnchangedFillAsync)
    ,("refuses a fill when the document forbids form filling", RefusesAForbiddenFillAsync)
    ,("fills a form even when content editing is forbidden", FillsWhenOnlyContentEditingIsForbiddenAsync)
    ,("renames a field and refreshes its page", RenamesAFormFieldAsync)
    ,("moves a field with structural permission and a current rect", MovesAFormFieldAsync)
    ,("resizes a field with valid dimensions and structural permission", ResizesAFormFieldAsync)
    ,("restyles a field without losing its other style attributes", RestylesAFormFieldAsync)
    ,("refuses a stale or forbidden field rename", RefusesInvalidFormRenameAsync)
    ,("edits form fields when only a full rewrite is refused", EditsFormFieldsWhenOnlyAFullRewriteIsRefusedAsync)
    ,("refuses a fill for a field that is no longer there", RefusesAFillForAMissingFieldAsync)
    ,("refreshes the preview on history after a fill", RefreshesThePreviewOnHistoryAfterAFillAsync)
    ,("maps a dropdown choice to its list index and back", MapsDropdownChoicesToIndices)
    ,("hands the core multiline text with the line breaks it splits on", NormalizesMultilineFieldText)
    ,("refuses page content once the session is retired", RefusesPageContentAfterSessionSwapAsync)
    ,("picks the smallest text run under a content-edit click", PicksTheSmallestRunUnderTheClick)
    ,("matches a PDF font name to a local face", MatchesPdfFontsToLocalFaces)
    ,("opening an editor closes the one already open", OpeningAnEditorClosesTheOneAlreadyOpen)
    ,("drains a write started while the commit was waiting", CommitDrainsAWriteStartedWhileItWasWaiting)
    ,("keeps the box open when a write the commit never saw is refused", ARefusalOnAWriteTheCommitNeverSawKeepsTheBoxOpen)
    ,("lands the writes then closes the box, before history moves", SettlingForHistoryLandsTheWritesThenClosesTheBox)
    ,("leaves nothing that could write the undone text back", AHistoryStepLeavesNothingThatCouldWriteTheUndoneTextBack)
    ,("asks for an undo when abandoning an edit it created", AbandoningAnEditThisSessionCreatedAsksForAnUndo)
    ,("writes the earlier text back when abandoning on top of it", AbandoningAnEditOnTopOfAnEarlierOneWritesTheEarlierTextBack)
    ,("stops the pump without latching a refusal when the document goes", ADocumentGoingAwayStopsThePumpWithoutLatchingARefusal)
    ,("sends only the latest text typed during a write", OnlyTheLatestTextIsSentWhenKeystrokesArriveDuringAWrite)
    ,("lets the pump try again once refused text is retyped", RetypingAfterARefusalLetsThePumpTryAgain)
    ,("stops the typing pause before a commit waits", CommittingStopsThePauseBeforeItWaits)
    ,("aims the core at the PDFium shipped beside the app", AimsTheCoreAtTheBundledPdfium)
    ,("leaves an operator's PDFium override alone", LeavesAnExistingPdfiumOverrideAlone)
    ,("leaves resolution to the core when nothing is bundled", LeavesResolutionToTheCoreWithoutABundledPdfium)
    ,("moves a page and hands back the new layout", MovesAPageAndHandsBackTheNewLayoutAsync)
    ,("appends a blank page and records unsaved work", AppendsABlankPageAsync)
    ,("inserts a blank page before a chosen page", InsertsABlankPageBeforeAChosenPageAsync)
    ,("inserts a landscape A4 page", InsertsALandscapeBlankPageAsync)
    ,("refuses blank page insertion when page assembly is forbidden", RefusesBlankPageInsertionAsync)
    ,("rotates a page by a quarter turn", RotatesAPageAsync)
    ,("removes a page and keeps the current page in range", RemovesAPageAndKeepsTheCurrentPageInRangeAsync)
    ,("refuses to remove a document's only page", RefusesToRemoveTheOnlyPageAsync)
    ,("explains a document that forbids page changes", ExplainsADocumentThatForbidsPageChangesAsync)
    ,("counts a page edit as unsaved work", CountsAPageEditAsUnsavedWorkAsync)
    ,("refreshes the preview on history after a page edit", RefreshesThePreviewOnHistoryAfterAPageEditAsync)
    ,("reports the current layout after history moves pages", ReportsTheCurrentLayoutAfterHistoryAsync)
    ,("refuses page edits once the session is retired", RefusesPageEditsAfterSessionSwapAsync)
    ,("inserts page text with its own font and undoable preview", InsertsContentTextAsync)
    ,("refuses invalid, stale or forbidden text insertion", RefusesInvalidTextInsertionAsync)
    ,("keeps text insertion undoable when preview fails", KeepsTextInsertionAfterPreviewFailureAsync)
};

foreach (var test in tests)
{
    await test.Run();
    Console.WriteLine($"PASS {test.Name}");
}

static Task StepsThroughSearchHits()
{
    Assert(SearchSelection.StepIndex(-1, 0, 1) == -1, "an empty search has no selection");
    Assert(SearchSelection.StepIndex(-1, 3, 1) == 0, "next starts at the first hit");
    Assert(SearchSelection.StepIndex(-1, 3, -1) == 2, "previous starts at the last hit");
    Assert(SearchSelection.StepIndex(0, 3, -1) == 2, "previous wraps to the last hit");
    Assert(SearchSelection.StepIndex(2, 3, 1) == 0, "next wraps to the first hit");
    Assert(SearchSelection.StepIndex(1, 3, 1) == 2, "next moves to the adjacent hit");
    return Task.CompletedTask;
}

static Task ReportsSelectedSearchHit()
{
    Assert(SearchSelection.Status("needle", 0, 3) == "Match 1 of 3 for \"needle\".", "the first hit is numbered from one");
    Assert(SearchSelection.Status("needle", SearchSelection.StepIndex(0, 3, -1), 3) == "Match 3 of 3 for \"needle\".", "wrapped navigation reports the selected hit");
    return Task.CompletedTask;
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

// A zero-page PDF is still something a user can *open*, so the empty-state
// mapping stays — it just no longer describes documents this app creates.
static async Task MapsBlankRenderToEmptyAsync()
{
    var core = new FakeCore { PageCount = 0, RenderError = PdfCoreError.DocumentNotFound };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("no-pages.pdf", [1]))).Value!;
    var result = await facade.RenderCurrentPageAsync(session.SessionId, 144, false);
    Assert(result.IsEmpty, "zero-page document render should be an empty state");
}

static async Task CreatesADocumentWithAPageToWorkOnAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.CreateBlankAsync()).Value!;

    Assert(session.PageCount >= 1, "a newly created document should arrive with a page to work on");
    Assert(session.State == DocumentSessionState.Ready, "a newly created document should not open in the empty state");

    var render = await facade.RenderCurrentPageAsync(session.SessionId, 144, false);
    Assert(render.IsSuccess, "the first page of a newly created document should render");
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

static async Task CarriesPageRotationOnTheSessionAsync()
{
    var core = new FakeCore { PageCount = 1, PageWidthPt = 842, PageHeightPt = 595, PageRotation = PageRotation.Clockwise90 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    // The size alone cannot tell a landscape page from a portrait one turned
    // on its side, and the overlays on the two go in different places.
    Assert(session.Pages[0].Rotation == PageRotation.Clockwise90, "the page's turn must reach the shell with its size");
}

static Task AsksTheCoreWherePageGeometryLandsAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var page = new PagePlacement(842, 595, PageRotation.Clockwise270, 1.5);

    var rect = facade.PlaceRect(new AnnotationRect(72, 100, 200, 20), page);
    var point = facade.PlacePoint(new AnnotationPoint(72, 100), page);
    var pdf = facade.PointToPdf(new PlacedPoint(10, 20), page);

    // The per-turn arithmetic is the core's; the facade only carries it, so
    // every answer here is the fake's and every argument reaches it as given.
    Assert(core.PlacementRequests.SequenceEqual([page, page, page]), "the page placement must reach the core unchanged");
    Assert(rect == new PlacedRect(1, 2, 3, 4), "the core's placed rect must be used as-is");
    Assert(point == new PlacedPoint(72 + 1, 100 + 1), "the core's placed point must be used as-is");
    Assert(pdf == new AnnotationPoint(10 - 1, 20 - 1), "the core's page-space point must be used as-is");
    return Task.CompletedTask;
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

static async Task RendersExportPagesWithoutNavigatingAsync()
{
    var core = new FakeCore { PageCount = 3 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    for (uint index = 0; index < session.PageCount; index++)
    {
        var result = await facade.RenderPageForPrintAsync(session.SessionId, index, 150, false);
        Assert(result.IsSuccess && result.Value!.PageIndex == index, "each export page must render independently");
        Assert(result.Value!.Rgba.Length == result.Value.Stride * result.Value.Height, "PNG encoder needs packed pixel rows");
    }
    Assert(core.RenderDpis.SequenceEqual([150u, 150u, 150u]), "each page must render at export resolution");
    var current = await facade.RenderCurrentPageAsync(session.SessionId, 96, false);
    Assert(current.Value!.PageIndex == 0, "export must not change the viewer's current page");
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

static async Task LoadsPageCharactersAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var result = await facade.PageCharactersAsync(session.SessionId, 0);

    Assert(result.IsSuccess, "loading a live session's page characters should succeed");
    using var characters = result.Value!;
    Assert(characters.PageIndex == 0, "the handle should report the page it was loaded for");
    Assert(characters.CaretAt(100, 700) == 0, "the fake's single character should resolve to caret 0");
    Assert(characters.TextIn(0, 1) == "A", "text between carets should come from the fake's page");
    Assert(characters.RectsIn(0, 1) is [{ WidthPt: 12 }], "rects between carets should come from the fake's page");
}

static async Task RefusesPageCharactersAfterSessionSwapAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
    var first = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    await facade.OpenAsync(new DocumentSource("second.pdf", [1]));

    var result = await facade.PageCharactersAsync(first.SessionId, 0);

    Assert(!result.IsSuccess, "a session id from before the last open should be refused");
}

// The three branches of the packaged app's PDFium resolution. They matter
// because the core's own fallbacks cannot serve a shipped build: its second
// step is a compile-time path into the build machine's vendor tree, so a
// package that forgets to name its own copy still renders on the machine that
// built it and nowhere else (T-064).
static Task AimsTheCoreAtTheBundledPdfium()
{
    var resolved = BundledPdfium.ResolveOverride(
        currentOverride: null,
        baseDirectory: @"C:\Program Files\Vitela",
        fileExists: path => path == @"C:\Program Files\Vitela\pdfium.dll");

    Assert(resolved == @"C:\Program Files\Vitela\pdfium.dll", "the copy beside the executable should be named explicitly");
    return Task.CompletedTask;
}

static Task LeavesAnExistingPdfiumOverrideAlone()
{
    var resolved = BundledPdfium.ResolveOverride(
        currentOverride: @"D:\ci\pdfium.dll",
        baseDirectory: @"C:\Program Files\Vitela",
        fileExists: _ => true);

    Assert(resolved is null, "an override already in the environment is how CI aims the loader and must win");
    return Task.CompletedTask;
}

static Task LeavesResolutionToTheCoreWithoutABundledPdfium()
{
    var resolved = BundledPdfium.ResolveOverride(
        currentOverride: null,
        baseDirectory: @"D:\checkout\apps\windows\Pdf.Windows\bin\x64\Debug",
        fileExists: _ => false);

    Assert(resolved is null, "a development build without a bundled copy should fall through to the core's own resolution");
    return Task.CompletedTask;
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

static async Task RecordsAnnotationEditsInCoreHistoryAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var added = await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));
    Assert(added.IsSuccess && added.Value!.Annotations.Count == 1, "adding an annotation must publish the core snapshot");
    Assert(added.Value!.CanUndo && !added.Value.CanRedo, "a new edit enables undo and clears redo");
    var undone = await facade.UndoAsync(session.SessionId);
    Assert(undone.IsSuccess && !undone.Value!.CanUndo && undone.Value.CanRedo, "undo must publish its updated history state");
}

static async Task PublishesRestyledAnnotationColorAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var added = await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));
    var restyled = await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Restyle(added.Value!.Annotations[0].Id, new PdfCoreColor(0, 128, 255)));

    Assert(restyled.IsSuccess, "restyling a selected annotation should succeed");
    Assert(restyled.Value!.Annotations[0].Color == new AnnotationColor(0, 128, 255), "the updated annotation state must expose the selected color");
}

static async Task RefusesForbiddenAnnotationEditsAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("restricted.pdf", [1]))).Value!;
    core.LastDocument!.EditingAllowed = false;
    var result = await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));
    Assert(!result.IsSuccess, "a permission-denied document must reject annotation edits");
    Assert(result.Error!.Message == "This document or action is not supported.", "permission refusal must remain user safe");
}

static async Task HoldsEditsUntilDestinationReplacementCompletesAsync()
{
    var core = new FakeCore { BlockSave = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var replacementStarted = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
    var releaseReplacement = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
    var save = facade.SaveToDestinationAsync(session.SessionId, async _ =>
    {
        replacementStarted.SetResult();
        await releaseReplacement.Task;
    });
    await core.SaveStarted.Task;
    core.ReleaseSave.Set();
    await replacementStarted.Task;
    var edit = facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));
    Assert(!edit.IsCompleted, "an annotation edit must wait until the destination replacement completes");
    releaseReplacement.SetResult();
    Assert((await save).IsSuccess, "a save should publish only after destination replacement succeeds");
    Assert((await edit).IsSuccess, "the queued annotation edit should run after the save boundary");
}

static async Task BlocksOpenWithUnsavedAnnotationsAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));
    var open = await facade.OpenAsync(new DocumentSource("second.pdf", [2]));
    Assert(!open.IsSuccess, "opening another document must not discard unsaved annotation edits");
    Assert(open.Error!.Message == "Save or undo the pending annotation changes before opening another document.", "the user must be told a way out that the shell actually offers");
}

/// <summary>
/// The revision counter climbs on undo as well, so it can never fall back to
/// the saved revision on its own — before the edit log was consulted, undoing
/// every change left the reader permanently unable to open anything else.
/// </summary>
static async Task ReleasesTheGuardOnceEditsAreUndoneAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));
    var undone = await facade.UndoAsync(session.SessionId);
    Assert(undone.IsSuccess && !undone.Value!.CanUndo, "the edit log must be empty for this to prove anything");
    var open = await facade.OpenAsync(new DocumentSource("second.pdf", [2]));
    Assert(open.IsSuccess, "with nothing left to undo there is no work to lose, so the open must proceed");
}

/// <summary>
/// The other half of the pair: the undo stack survives a save, so asking it
/// alone would keep refusing long after the file on disk held every edit.
/// </summary>
static async Task ReleasesTheGuardOnceEditsAreSavedAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));
    var saved = await facade.SaveToDestinationAsync(session.SessionId, _ => Task.CompletedTask);
    Assert(saved.IsSuccess, "the save must land for this to prove anything");
    var state = await facade.AnnotationStateAsync(session.SessionId);
    Assert(state.Value!.CanUndo, "the edit log must still hold the saved edit for this to prove anything");
    var open = await facade.OpenAsync(new DocumentSource("second.pdf", [2]));
    Assert(open.IsSuccess, "a saved document has no pending work, however full its undo stack");
}

static Task KeepsStampPreviewsScopedToSession()
{
    var previews = new StampPreviewCache<string>();
    Assert(previews.BeginSession("first"), "the first document session must initialize the cache");
    previews.Set("first", 7, "preview");
    Assert(previews.TryGet(7, out var preview) && preview == "preview", "a preview must be available during its session");
    previews.Set("stale", 8, "ignored");
    Assert(!previews.TryGet(8, out _), "a stale operation must not publish into the current cache");
    Assert(!previews.BeginSession("first"), "reusing a session must preserve undo/redo previews");
    Assert(previews.TryGet(7, out _), "the same session must retain its preview");
    Assert(previews.BeginSession("second"), "a replacement document must start a new cache");
    Assert(!previews.TryGet(7, out _), "a replacement document must clear the old previews");
    return Task.CompletedTask;
}

static Task StepsBackwardThroughAnnotations()
{
    var first = new Annotation(1, 0, AnnotationKind.Shape, new AnnotationRect(0, 0, 10, 10), null, []);
    var second = first with { Id = 2 };
    var third = first with { Id = 3 };
    Annotation[] annotations = [first, second, third];

    Assert(AnnotationSelection.PreviousId(annotations, 3) == 2, "selection should step backward in document order");
    Assert(AnnotationSelection.PreviousId(annotations, 1) == 3, "the first annotation should wrap to the last");
    Assert(AnnotationSelection.PreviousId(annotations, 9) is null, "a deleted selection should not select another annotation");
    Assert(AnnotationSelection.PreviousId(annotations, null) is null, "navigation needs a selected annotation");
    Assert(AnnotationSelection.PreviousId([], 1) is null, "an empty document has no previous annotation");
    Assert(annotations[0].Id == 1 && annotations[1].Id == 2, "navigation must not reorder the document");
    return Task.CompletedTask;
}

static Task ReconcilesInsertedStamp()
{
    var existing = new Annotation(1, 0, AnnotationKind.Shape, new AnnotationRect(0, 0, 10, 10), null, []);
    var stamp = new Annotation(2, 0, AnnotationKind.Stamp, new AnnotationRect(10, 10, 20, 20), null, []);
    Assert(StampPreviewReconciliation.InsertedStampId([existing], [existing, stamp]) == stamp.Id, "the new stamp ID must receive the decoded preview");
    Assert(StampPreviewReconciliation.InsertedStampId([existing], [existing]) is null, "a non-insertion snapshot must not claim a preview");
    var secondStamp = new Annotation(3, 0, AnnotationKind.Stamp, new AnnotationRect(20, 20, 20, 20), null, []);
    Assert(StampPreviewReconciliation.InsertedStampId([existing], [existing, stamp, secondStamp]) is null, "an ambiguous snapshot must keep the rectangle fallback");
    return Task.CompletedTask;
}

static Task RejectsStaleStampInputSession()
{
    Assert(ImageStampInput.SessionMatches("active", "active"), "the current session may insert a stamp");
    Assert(!ImageStampInput.SessionMatches("retired", "active"), "a replaced session must not mutate the current document");
    Assert(!ImageStampInput.SessionMatches("retired", null), "a closed document must not accept in-flight input");
    return Task.CompletedTask;
}

static Task RoutesSupportedStampSignatures()
{
    Assert(ImageStampInput.HasSupportedFileExtension("stamp.PNG"), "PNG extensions must be accepted case-insensitively");
    Assert(ImageStampInput.HasSupportedFileExtension("stamp.jpeg"), "JPEG extensions must be accepted");
    Assert(ImageStampInput.HasSupportedSignature([137, 80, 78, 71, 13, 10, 26, 10]), "a PNG signature must route to insertion");
    Assert(ImageStampInput.HasSupportedSignature([0xff, 0xd8, 0xff, 0xe0]), "a JPEG signature must route to insertion");
    return Task.CompletedTask;
}

static Task RejectsUnsupportedStampInput()
{
    Assert(!ImageStampInput.HasSupportedFileExtension("stamp.gif"), "only PNG and JPEG local files are accepted");
    Assert(!ImageStampInput.HasSupportedSignature("not an image"u8), "non-image bytes must not reach the core");
    Assert(ImageStampInput.HasSupportedSignature([137, 80, 78, 71, 13, 10, 26, 10, 1]), "recognized but corrupt images must reach the core validator");
    return Task.CompletedTask;
}

static async Task ReportsInvalidStampImageAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore { InsertStampError = PdfCoreError.InvalidImage }, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var result = await facade.InsertStampAsync(session.SessionId, 0, [137, 80, 78, 71, 13, 10, 26, 10], new PdfCoreRect(10, 20, 30, 40));
    Assert(!result.IsSuccess, "the core must reject corrupt image content");
    Assert(result.Error!.Message == "The image is corrupt or unsupported.", "core image failures must have clear user feedback");
}

static Task AsksTheCoreWhereAStampGoesAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());

    var result = facade.StampPlacement([137, 80, 78, 71, 13, 10, 26, 10], 200, 500);

    Assert(result.IsSuccess, "placement is pure geometry and needs no open session");
    Assert(core.StampPlacementAnchors.TryDequeue(out var anchor) && anchor == (200, 500), "the drop point must reach the core unchanged");
    // The size is whatever the core says. A shell that sized stamps itself
    // would produce its own constant here and drift from the GTK shell.
    Assert(result.Value! == new PdfCoreRect(200, 500 - 77, 77, 77), "the core's rect must be used as-is");
    return Task.CompletedTask;
}

static Task ReportsAnUnplaceableStampImage()
{
    using var facade = new PdfDocumentFacade(new FakeCore { StampPlacementError = PdfCoreError.InvalidImage }, new RecordingLogger());

    var result = facade.StampPlacement("not an image"u8.ToArray(), 0, 0);

    Assert(!result.IsSuccess, "bytes that cannot be measured cannot be placed");
    Assert(result.Error!.Message == "The image is corrupt or unsupported.", "placement failures must read the same as insert failures");
    return Task.CompletedTask;
}

static Task RoutesDroppedFilesByKind()
{
    Assert(FileDropRouting.Classify("report.pdf") == DroppedFileKind.Document, "a dropped PDF must open as the document");
    Assert(FileDropRouting.Classify("REPORT.PDF") == DroppedFileKind.Document, "extensions must be matched case-insensitively");
    Assert(FileDropRouting.Classify(@"C:\stamps\signature.png") == DroppedFileKind.ImageStamp, "a dropped image must be placed as a stamp");
    Assert(FileDropRouting.Classify("scan.jpeg") == DroppedFileKind.ImageStamp, "JPEG shares the image stamp route");
    Assert(FileDropRouting.Classify("notes.txt") == DroppedFileKind.Unsupported, "an unrelated file must be refused rather than guessed at");
    Assert(FileDropRouting.Classify("") == DroppedFileKind.Unsupported, "a path-less drop entry must be refused");
    Assert(FileDropRouting.Classify(null) == DroppedFileKind.Unsupported, "a path-less drop entry must be refused");
    return Task.CompletedTask;
}

static Task ActsOnOneDroppedFile()
{
    string[] paths = ["README.md", "first.pdf", "second.pdf", "logo.png"];
    var chosen = FileDropRouting.FirstActionable(paths, path => path);
    Assert(chosen is { Item: "first.pdf", Kind: DroppedFileKind.Document }, "unsupported entries are skipped and the first actionable file wins");
    Assert(FileDropRouting.FirstActionable(["a.txt", "b.zip"], path => path) is null, "a drop with nothing actionable must report no choice");
    Assert(FileDropRouting.FirstActionable(Array.Empty<string>(), path => path) is null, "an empty drop must report no choice");
    return Task.CompletedTask;
}

/// <summary>
/// The whole point of the correlation id: the user reports it, and somebody
/// can find the entry it refers to. Before the log existed, the detail went
/// only to `Debug.WriteLine` — invisible without a debugger, and absent from a
/// Release build.
/// </summary>
static async Task WritesRetrievableDiagnosticsAsync()
{
    var path = Path.Combine(Path.GetTempPath(), $"vitela-diag-{Guid.NewGuid():N}", "diagnostics.log");
    try
    {
        using var facade = new PdfDocumentFacade(new FakeCore { OpenError = PdfCoreError.Io }, new FileDiagnosticLogger(path));
        var result = await facade.OpenAsync(new DocumentSource("broken.pdf", [1]));
        Assert(!result.IsSuccess, "the open must fail for this to prove anything");

        var written = await File.ReadAllTextAsync(path);
        Assert(written.Contains(result.Error!.CorrelationId, StringComparison.Ordinal), "the reference shown to the user must appear in the log");
        Assert(written.Contains("Io", StringComparison.Ordinal), "the failure category must be recoverable from the log");
        Assert(written.Contains("open", StringComparison.Ordinal), "the operation must be recoverable from the log");
        Assert(!written.Contains("broken.pdf", StringComparison.Ordinal), "the document name must not reach the log");
    }
    finally
    {
        if (Path.GetDirectoryName(path) is { } directory && Directory.Exists(directory)) Directory.Delete(directory, recursive: true);
    }
}

static Task CapsTheDiagnosticLog()
{
    var directory = Path.Combine(Path.GetTempPath(), $"vitela-diag-{Guid.NewGuid():N}");
    var path = Path.Combine(directory, "diagnostics.log");
    try
    {
        var logger = new FileDiagnosticLogger(path, maxBytes: 512);
        for (var i = 0; i < 40; i++)
        {
            logger.Failure(PdfCoreError.Internal, "render", $"correlation{i}", "session", 0, "typed_failure");
        }

        Assert(new FileInfo(path).Length < 512, "the live log must stay under its cap");
        Assert(File.Exists(path + ".1"), "one generation back must survive rotation");
        Assert(File.ReadAllText(path).Contains("correlation39", StringComparison.Ordinal), "the most recent failure must be in the live log");
    }
    finally
    {
        if (Directory.Exists(directory)) Directory.Delete(directory, recursive: true);
    }
    return Task.CompletedTask;
}

/// <summary>A log that cannot be written must not become the failure it exists to explain.</summary>
static Task SwallowsDiagnosticWriteFailures()
{
    // An existing *directory* where the log file should go: creating the
    // parent succeeds, appending to it cannot.
    var directory = Path.Combine(Path.GetTempPath(), $"vitela-diag-{Guid.NewGuid():N}");
    var path = Path.Combine(directory, "diagnostics.log");
    Directory.CreateDirectory(path);
    try
    {
        new FileDiagnosticLogger(path).Failure(PdfCoreError.Internal, "save", "correlation", null, null, "typed_failure");
    }
    finally
    {
        if (Directory.Exists(directory)) Directory.Delete(directory, recursive: true);
    }
    return Task.CompletedTask;
}

/// <summary>
/// The flag is what tells the shell to prompt rather than report a dead end;
/// without it the refusal is indistinguishable from a corrupt file.
/// </summary>
static async Task FlagsPendingEditDecisionAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));

    var refused = await facade.OpenAsync(new DocumentSource("second.pdf", [2]));
    Assert(!refused.IsSuccess, "the open must still be refused by default");
    Assert(refused.Error!.RequiresPendingEditDecision, "the shell must be able to tell this refusal from a dead end");
    Assert(!refused.Error.RequiresPassword, "a pending-edit refusal is not a password prompt");

    using var other = new PdfDocumentFacade(new FakeCore { OpenError = PdfCoreError.Io }, new RecordingLogger());
    var broken = await other.OpenAsync(new DocumentSource("broken.pdf", [1]));
    Assert(!broken.Error!.RequiresPendingEditDecision, "an unrelated failure must not offer to discard anything");
}

static async Task DiscardsPendingEditsOnRequestAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));

    var opened = await facade.OpenAsync(new DocumentSource("second.pdf", [2]), discardPendingEdits: true);
    Assert(opened.IsSuccess, "an explicit discard must get past the guard");
    Assert(opened.Value!.SessionId != session.SessionId, "the replacement document must be a new session");

    var edits = await facade.AnnotationStateAsync(opened.Value.SessionId);
    Assert(edits.IsSuccess && edits.Value!.Annotations.Count == 0, "the discarded work must not follow the reader into the new document");
}

/// <summary>
/// Pins the order the shell's open path is built on: while edits are pending,
/// the guard answers before the core sees the bytes, so an encrypted file only
/// asks for its password on the retry — and that retry must take the password
/// and the discard together. A wrong password must not cost the edits.
/// </summary>
static async Task UnlocksOverDiscardedPendingEditsAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore { RequiredPassword = "secret" }, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]), "secret")).Value!;
    await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));

    var refused = await facade.OpenAsync(new DocumentSource("locked.pdf", [2]));
    Assert(refused.Error!.RequiresPendingEditDecision && !refused.Error.RequiresPassword, "pending edits must be asked about before the password");

    var locked = await facade.OpenAsync(new DocumentSource("locked.pdf", [2]), discardPendingEdits: true);
    Assert(locked.Error!.RequiresPassword, "past the guard, an encrypted file must ask for its password");

    var wrong = await facade.OpenAsync(new DocumentSource("locked.pdf", [2]), "nope", discardPendingEdits: true);
    Assert(wrong.Error!.RequiresPassword, "a wrong password must still be a password prompt");
    var kept = await facade.AnnotationStateAsync(session.SessionId);
    Assert(kept.IsSuccess && kept.Value!.Annotations.Count == 1, "a failed unlock must leave the pending edits in place");

    var opened = await facade.OpenAsync(new DocumentSource("locked.pdf", [2]), "secret", discardPendingEdits: true);
    Assert(opened.IsSuccess, "the password and the discard together must open the document");
    Assert(opened.Value!.SessionId != session.SessionId, "the unlocked document must be a new session");
}

static async Task BlocksCreateBlankWithUnsavedAnnotationsAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));
    var created = await facade.CreateBlankAsync();
    Assert(!created.IsSuccess, "creating a blank document must not discard unsaved annotation edits");
    Assert(created.Error!.RequiresPendingEditDecision, "the shell must be able to offer Save/Discard/Cancel for this refusal");
}

static async Task CreatesBlankDocumentOverPendingEditsOnRequestAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));

    var created = await facade.CreateBlankAsync(discardPendingEdits: true);
    Assert(created.IsSuccess, "an explicit discard must get past the guard");
    Assert(created.Value!.SessionId != session.SessionId, "the blank document must be a new session");

    var edits = await facade.AnnotationStateAsync(created.Value.SessionId);
    Assert(edits.IsSuccess && edits.Value!.Annotations.Count == 0, "the discarded work must not follow the reader into the blank document");
}

static async Task MapsUnexpectedSaveFailureAsync()
{
    using var facade = new PdfDocumentFacade(new FakeCore { SaveThrowsUnexpected = true }, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var result = await facade.SaveToDestinationAsync(session.SessionId, _ => Task.CompletedTask);
    Assert(!result.IsSuccess && result.Error!.Message == "The document could not be processed.", "unexpected save failures must be user safe");
}

/// <summary>
/// A caller that never asked about the signature must not get a file with a
/// broken one — and what it is told has to be about the signature. Folding it
/// into "the document could not be processed" would read like a bug in the
/// app rather than a choice the reader still has.
/// </summary>
static async Task RefusesToSilentlyBreakASignatureAsync()
{
    var core = new FakeCore { SignedDocument = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("signed.pdf", [1]))).Value!;

    var wrote = false;
    var result = await facade.SaveToDestinationAsync(session.SessionId, _ =>
    {
        wrote = true;
        return Task.CompletedTask;
    });

    Assert(!result.IsSuccess, "an unacknowledged save of a signed document must fail");
    Assert(result.Error!.Message.Contains("signature"), "the user must be told what is actually at stake, not that the document could not be processed");
    Assert(!wrote, "nothing may reach the destination");
    Assert(core.LastSaveAcknowledgedSignatures == false, "the default must never acknowledge on the reader's behalf");
}

/// <summary>
/// The question the shell asks so it can put the warning in front of the
/// reader instead of letting them find out through a failed save.
/// </summary>
static async Task ReportsWhetherSavingBreaksASignatureAsync()
{
    using var signed = new PdfDocumentFacade(new FakeCore { SignedDocument = true }, new RecordingLogger());
    var signedSession = (await signed.OpenAsync(new DocumentSource("signed.pdf", [1]))).Value!;
    var signedAnswer = await signed.WillInvalidateSignaturesAsync(signedSession.SessionId);
    Assert(signedAnswer.IsSuccess && signedAnswer.Value, "a signed document must be reported as at risk");

    using var plain = new PdfDocumentFacade(new FakeCore(), new RecordingLogger());
    var plainSession = (await plain.OpenAsync(new DocumentSource("plain.pdf", [1]))).Value!;
    var plainAnswer = await plain.WillInvalidateSignaturesAsync(plainSession.SessionId);
    Assert(plainAnswer.IsSuccess && !plainAnswer.Value, "an unsigned document has nothing to warn about");

    var gone = await plain.WillInvalidateSignaturesAsync("no-such-session");
    Assert(!gone.IsSuccess, "a session that is gone cannot be answered for");
}

/// <summary>
/// Once the reader has been shown what they lose and said yes, the save is
/// theirs to make — the acknowledgement has to actually reach the core, or
/// the dialog would be theatre and signed documents would stay unsaveable.
/// </summary>
static async Task SavesASignedDocumentOnceAcknowledgedAsync()
{
    var core = new FakeCore { SignedDocument = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("signed.pdf", [1]))).Value!;

    var wrote = false;
    var result = await facade.SaveToDestinationAsync(session.SessionId, _ =>
    {
        wrote = true;
        return Task.CompletedTask;
    }, signaturesAcknowledged: true);

    Assert(result.IsSuccess, "an acknowledged save must go through");
    Assert(wrote, "the bytes must reach the destination");
    Assert(core.LastSaveAcknowledgedSignatures == true, "the acknowledgement must reach the core, not stop at the facade");
}

static async Task ProtectsWithTwoDistinctPasswordRolesAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    byte[]? written = null;

    var result = await facade.ProtectToDestinationAsync(
        session.SessionId,
        "open-pw",
        "permissions-pw",
        bytes => { written = bytes; return Task.CompletedTask; });

    Assert(result.IsSuccess && written is not null, "protected bytes must reach the selected destination");
    Assert(core.LastProtectionPasswords == ("open-pw", "permissions-pw"), "both password roles must reach the core unchanged");
}

static async Task RefusesForbiddenProtectionAsync()
{
    var core = new FakeCore { ContentEditingPermitted = false };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var result = await facade.ProtectToDestinationAsync(
        session.SessionId,
        "open-pw",
        "permissions-pw",
        _ => Task.CompletedTask);

    Assert(!result.IsSuccess, "protection must use the general content-modification permission");
    Assert(core.LastProtectionPasswords is null, "a refused request must not reach the core");
}

static async Task CompressesWithTheChosenPresetAsync()
{
    var core = new FakeCore { CompressionOutput = new PdfCoreCompressedSave([7], 2_400_000, 840_000, 1_560_000, true, []) };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var result = await facade.CompressAsync(session.SessionId, CompressionPreset.Small);

    Assert(result.IsSuccess, "an unsigned document must compress");
    Assert(core.LastCompressPreset == PdfCoreCompressPreset.Small, "the chosen preset must reach the core unchanged");
    var compressed = result.Value!;
    Assert(compressed.Bytes.SequenceEqual(new byte[] { 7 }), "the compressed bytes must be the core's");
    Assert(compressed.BeforeBytes == 2_400_000 && compressed.AfterBytes == 840_000 && compressed.SavedBytes == 1_560_000, "both sizes must be the core's measurement, not re-derived");
    Assert(compressed.Reduced, "the outcome must be carried, not guessed from the sizes");
}

static async Task RefusesToSilentlyBreakASignatureWhenCompressingAsync()
{
    var core = new FakeCore { SignedDocument = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var asked = await facade.CompressionWillInvalidateSignaturesAsync(session.SessionId);
    var result = await facade.CompressAsync(session.SessionId, CompressionPreset.Balanced);

    Assert(asked.IsSuccess && asked.Value, "the compression's own signature question must answer true for a signed file");
    Assert(!result.IsSuccess, "an unacknowledged compression of a signed file must be refused");
    Assert(core.LastCompressAcknowledgedSignatures == false, "the facade must not acknowledge on the reader's behalf");
}

static async Task CompressesASignedDocumentOnceAcknowledgedAsync()
{
    var core = new FakeCore { SignedDocument = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var result = await facade.CompressAsync(session.SessionId, CompressionPreset.Lossless, signaturesAcknowledged: true);

    Assert(result.IsSuccess, "an acknowledged compression must go through");
    Assert(core.LastCompressAcknowledgedSignatures == true, "the acknowledgement must reach the core");
}

static async Task ReportsWhyADocumentCannotBeCompressedAsync()
{
    var core = new FakeCore { CompressionBlocker = "this document's password protection does not allow it to be rewritten, so it cannot be compressed" };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var refusal = await facade.CompressionRefusalAsync(session.SessionId);

    Assert(refusal.IsSuccess, "asking the gate must not itself fail");
    Assert(refusal.Value == "This document's password protection does not allow it to be rewritten, so it cannot be compressed.",
        $"the core's own sentence must reach the reader, capitalised and stopped, got '{refusal.Value}'");
}

static ImageExportRequest ExportRequest(
    ImageExportPages pages = ImageExportPages.All,
    string customRange = "",
    uint currentPage = 0,
    uint dpi = ImageExportLimits.DefaultDpi,
    ImageExportFormat format = ImageExportFormat.Png) => new(pages, customRange, currentPage, dpi, format);

static async Task PlansAnImageExportOfEveryPageAsync()
{
    var core = new FakeCore { PageCount = 3 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var plan = await facade.PlanImageExportAsync(session.SessionId, ExportRequest(format: ImageExportFormat.Jpeg));

    Assert(plan.IsSuccess, "an ordinary export must plan");
    Assert(plan.Value!.Files.Select(file => file.PageIndex).SequenceEqual([0u, 1u, 2u]), "All must cover every page, in order");
    // The fake's name is shaped so no shell-side format string could produce
    // it: the file names must come from the core.
    Assert(plan.Value.Files[1].FileName == "report.pdf|1|3|Jpeg", $"the core must name the files, got '{plan.Value.Files[1].FileName}'");
    Assert(plan.Value.Format == ImageExportFormat.Jpeg && plan.Value.Dpi == ImageExportLimits.DefaultDpi, "the plan carries what was chosen");
}

static async Task PlansAnImageExportOfTheCurrentPageAsync()
{
    var core = new FakeCore { PageCount = 3 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var plan = await facade.PlanImageExportAsync(session.SessionId, ExportRequest(ImageExportPages.Current, currentPage: 1));

    Assert(plan.Value!.Files.Select(file => file.PageIndex).SequenceEqual([1u]), "Current must export the page being viewed and nothing else");
}

/// <summary>
/// A page removal can leave the viewer's page one past the end until it
/// catches up; exporting the last page beats exporting nothing.
/// </summary>
static async Task KeepsAStaleCurrentPageInsideTheDocumentAsync()
{
    var core = new FakeCore { PageCount = 3 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var plan = await facade.PlanImageExportAsync(session.SessionId, ExportRequest(ImageExportPages.Current, currentPage: 9));

    Assert(plan.Value!.Files.Select(file => file.PageIndex).SequenceEqual([2u]), "a stale current page must land on the last page");
}

static async Task ReadsACustomPageRangeWithTheCoresGrammarAsync()
{
    var core = new FakeCore { PageCount = 10, ParsedSelection = [0, 2, 6] };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var plan = await facade.PlanImageExportAsync(session.SessionId, ExportRequest(ImageExportPages.Custom, "1,3,7"));

    Assert(core.LastSelection == ("1,3,7", 10u), "the typed range and the page count must reach the core as they are");
    Assert(plan.Value!.Files.Select(file => file.PageIndex).SequenceEqual([0u, 2u, 6u]), "the core's pages must be the ones exported");
}

static async Task ShowsTheCoresSentenceForABadPageRangeAsync()
{
    var core = new FakeCore { PageCount = 10, SelectionRefusal = "The range 7-3 runs backwards; write it as 3-7." };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var plan = await facade.PlanImageExportAsync(session.SessionId, ExportRequest(ImageExportPages.Custom, "7-3"));

    Assert(!plan.IsSuccess, "a range the core refused must not plan");
    Assert(plan.Error!.Message == "The range 7-3 runs backwards; write it as 3-7.", $"the core's sentence must reach the reader unchanged, got '{plan.Error.Message}'");
}

static async Task RefusesAForbiddenImageExportAsync()
{
    var core = new FakeCore { PageCount = 2, ExtractionPermitted = false };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var plan = await facade.PlanImageExportAsync(session.SessionId, ExportRequest());

    Assert(!plan.IsSuccess, "a document that withholds extraction must not plan an export");
    Assert(plan.Error!.Message == "This document does not permit extracting its pages as images.", plan.Error.Message);
    Assert(core.ExportedPages.IsEmpty, "nothing may be rendered");
}

static async Task RefusesAResolutionOutsideTheDialogAsync()
{
    var core = new FakeCore { PageCount = 1 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    foreach (var dpi in new[] { ImageExportLimits.MinDpi - 1, ImageExportLimits.MaxDpi + 1 })
    {
        var plan = await facade.PlanImageExportAsync(session.SessionId, ExportRequest(dpi: dpi));
        Assert(!plan.IsSuccess && plan.Error!.Message == "Choose a resolution between 72 and 400 DPI.", $"{dpi} DPI must be refused");
    }
}

static async Task NamesAnOversizedPageBeforeWritingAsync()
{
    var core = new FakeCore { PageCount = 3, OversizedPage = 1 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var plan = await facade.PlanImageExportAsync(session.SessionId, ExportRequest(dpi: 400));

    Assert(!plan.IsSuccess, "an export that would fail part-way must not start");
    Assert(plan.Error!.Message == "Page 2 is too large to export at 400 DPI. Choose a lower resolution.", plan.Error.Message);
    Assert(core.LastOversizeQuery is { } query && query.Pages.SequenceEqual([0u, 1u, 2u]) && query.Dpi == 400, "the core must be asked about exactly the planned pages at the chosen DPI");
}

static async Task ExportsAPagesEncodedBytesAsync()
{
    var core = new FakeCore { PageCount = 3 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var image = await facade.ExportPageImageAsync(session.SessionId, 2, 300, ImageExportFormat.Jpeg);

    Assert(image.IsSuccess && image.Value!.SequenceEqual(new byte[] { 0xFF, 0xD8 }), "the core's encoded bytes must come back untouched");
    Assert(core.ExportedPages.SequenceEqual([(2u, 300u, PdfCoreImageFormat.Jpeg)]), "page, DPI and format must reach the core");
}

static async Task RefusesAPageImageAfterSessionSwapAsync()
{
    var core = new FakeCore { PageCount = 1 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var first = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    await facade.OpenAsync(new DocumentSource("second.pdf", [2]));

    var image = await facade.ExportPageImageAsync(first.SessionId, 0, 150, ImageExportFormat.Png);

    Assert(!image.IsSuccess && image.Error!.Message == "The document is no longer available.", "a replaced session must not export");
    Assert(core.ExportedPages.IsEmpty, "nothing may be rendered for a replaced session");
}

static Task SummarisesAnImageExport()
{
    Assert(ImageExportWording.Summary(1, ImageExportFormat.Png, @"C:\out") == @"Exported 1 page as PNG to C:\out.", "singular");
    Assert(ImageExportWording.Summary(12, ImageExportFormat.Jpeg, @"C:\out") == @"Exported 12 pages as JPEG to C:\out.", "plural");
    return Task.CompletedTask;
}

static async Task PlansAnExtractionWithTheCoresParsedPagesAsync()
{
    var core = new FakeCore { PageCount = 10, ParsedSelection = [0, 2, 6] };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var plan = await facade.PlanExtractPagesAsync(session.SessionId, new ExtractPagesRequest("1,3,7"));

    Assert(core.LastSelection == ("1,3,7", 10u), "the typed range and the page count must reach the core as they are");
    Assert(plan.IsSuccess && plan.Value!.Pages.SequenceEqual([0u, 2u, 6u]), "the core's pages must be the ones planned");
}

static async Task RefusesAnEmptyExtractRangeAsync()
{
    var core = new FakeCore { PageCount = 5 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var plan = await facade.PlanExtractPagesAsync(session.SessionId, new ExtractPagesRequest("   "));

    Assert(!plan.IsSuccess, "an untyped range must not plan");
    Assert(plan.Error!.Message == "Type which pages to extract, for example 1-3,7.", plan.Error.Message);
    Assert(core.LastSelection is null, "the core's grammar must not even be asked about an empty range");
}

static async Task ShowsTheCoresSentenceForABadExtractRangeAsync()
{
    var core = new FakeCore { PageCount = 10, SelectionRefusal = "The range 7-3 runs backwards; write it as 3-7." };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var plan = await facade.PlanExtractPagesAsync(session.SessionId, new ExtractPagesRequest("7-3"));

    Assert(!plan.IsSuccess, "a range the core refused must not plan");
    Assert(plan.Error!.Message == "The range 7-3 runs backwards; write it as 3-7.", $"the core's sentence must reach the reader unchanged, got '{plan.Error.Message}'");
}

static async Task RefusesAnExtractionForbiddenByCopyingAsync()
{
    var core = new FakeCore { PageCount = 3, ExtractionPermitted = false };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var plan = await facade.PlanExtractPagesAsync(session.SessionId, new ExtractPagesRequest("1"));

    Assert(!plan.IsSuccess, "a document that withholds extraction must not plan one");
    Assert(plan.Error!.Message == "This document does not permit extracting its pages.", plan.Error.Message);
    Assert(core.LastSelection is null, "the grammar must not run once the permission gate has already refused");
}

static async Task RefusesAnExtractionThatCannotBeRewrittenAsync()
{
    var core = new FakeCore { PageCount = 3, RewriteAllowed = false };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var plan = await facade.PlanExtractPagesAsync(session.SessionId, new ExtractPagesRequest("1"));

    Assert(!plan.IsSuccess, "a document that cannot be fully rewritten must not plan an extraction");
    Assert(
        plan.Error!.Message == "Extracting pages rewrites the whole file, which this document's encryption or available credentials do not allow.",
        plan.Error.Message);
}

static async Task ExtractsPrunedBytesForThePlannedPagesAsync()
{
    var core = new FakeCore { PageCount = 5 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var extracted = await facade.ExtractPagesAsync(session.SessionId, [0, 2, 4]);

    Assert(extracted.IsSuccess && extracted.Value!.Length == 3, "the fake's stand-in bytes must be one per planned page");
    Assert(core.LastExtractedPages!.SequenceEqual([0u, 2u, 4u]), "exactly the planned pages must reach the core");
}

static async Task RefusesAnExtractionAfterSessionSwapAsync()
{
    var core = new FakeCore { PageCount = 1 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var first = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    await facade.OpenAsync(new DocumentSource("second.pdf", [2]));

    var extracted = await facade.ExtractPagesAsync(first.SessionId, [0]);

    Assert(!extracted.IsSuccess && extracted.Error!.Message == "The document is no longer available.", "a replaced session must not extract");
    Assert(core.LastExtractedPages is null, "nothing may be pruned for a replaced session");
}

static Task SummarisesAnExtraction()
{
    Assert(ExtractPagesWording.Summary(1, @"C:\out\part.pdf", false) == @"Extracted 1 page to C:\out\part.pdf.", "singular");
    Assert(ExtractPagesWording.Summary(3, @"C:\out\part.pdf", false) == @"Extracted 3 pages to C:\out\part.pdf.", "plural");
    return Task.CompletedTask;
}

static Task AddsASignatureNoteOnlyWhenTheSourceIsSigned()
{
    var unsigned = ExtractPagesWording.Summary(2, @"C:\out\part.pdf", false);
    var signed = ExtractPagesWording.Summary(2, @"C:\out\part.pdf", true);

    Assert(!unsigned.Contains("no longer verifies"), "an unsigned source earns no signature note");
    Assert(signed.StartsWith(@"Extracted 2 pages to C:\out\part.pdf.") && signed.Contains("no longer verifies"), signed);
    return Task.CompletedTask;
}

static async Task PlansSplitPartsAsync()
{
    var core = new FakeCore { PageCount = 10, ExtractedSourceSigned = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;

    var plan = await facade.PlanSplitPagesAsync(session.SessionId, "3,7");

    Assert(plan.IsSuccess && plan.Value!.SourceIsSigned, "signature must be reported");
    Assert(core.LastSplitRequest == ("3,7", 10u, "report.pdf"), "core must receive the typed cuts and name");
    Assert(plan.Value!.Parts.SequenceEqual([new SplitPart(0, 2, "report-part1.pdf"), new SplitPart(3, 9, "report-part2.pdf")]), "core part boundaries must remain intact");
}

static async Task RefusesForbiddenSplitAsync()
{
    foreach (var core in new[] { new FakeCore { PageCount = 3, ExtractionPermitted = false }, new FakeCore { PageCount = 3, RewriteAllowed = false } })
    {
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("report.pdf", [1]))).Value!;
        var plan = await facade.PlanSplitPagesAsync(session.SessionId, "1");
        Assert(!plan.IsSuccess && core.LastSplitRequest is null, "no forbidden document may reach split planning");
    }
}

static async Task RefusesSplitAfterSessionSwapAsync()
{
    var core = new FakeCore { PageCount = 4 };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var first = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    await facade.OpenAsync(new DocumentSource("second.pdf", [2]));

    var plan = await facade.PlanSplitPagesAsync(first.SessionId, "2");

    Assert(!plan.IsSuccess && core.LastSplitRequest is null, "a retired session must not plan files");
}

static Task SizesCompressionResultsInPowersOfTen()
{
    Assert(CompressionWording.HumanSize(2_400_000) == "2.4 MB", "megabytes");
    Assert(CompressionWording.HumanSize(840_000) == "840 kB", "kilobytes");
    Assert(CompressionWording.HumanSize(1_000) == "1 kB", "kilobytes start at their own boundary");
    Assert(CompressionWording.HumanSize(1_000_000) == "1.0 MB", "megabytes start at their own boundary");
    Assert(CompressionWording.HumanSize(999) == "999 bytes", "bytes");
    Assert(CompressionWording.HumanSize(1) == "1 byte", "singular");
    Assert(CompressionWording.HumanSize(0) == "0 bytes", "zero");
    return Task.CompletedTask;
}

static Task ReportsACompressionSavingWithoutRoundingUp()
{
    // 49.7% smaller must read 49%, a number the file actually reached.
    var result = new CompressionResult([1], 1_000, 503, 497, true, []);
    Assert(CompressionWording.PercentSmaller(result) == 49, "the percentage must truncate, not round");
    Assert(CompressionWording.PercentSmaller(new CompressionResult([], 0, 0, 0, false, [])) == 0, "an empty save must not divide by zero");
    var written = CompressionWording.WrittenSummary(result, @"C:\out\small.pdf");
    Assert(written == @"Compressed PDF written to C:\out\small.pdf (1 kB → 503 bytes, 49% smaller).", written);
    return Task.CompletedTask;
}

static Task SaysANoGainCompressionWroteNothing()
{
    var result = new CompressionResult([1], 2_400_000, 2_400_000, 0, false, ["a reason the core gave"]);
    var summary = CompressionWording.NoGainSummary(result);
    Assert(summary.StartsWith("Nothing to gain: this document compresses to the same 2.4 MB"), summary);
    Assert(summary.Contains("no file was written"), summary);
    Assert(summary.EndsWith(" Not everything could be done: a reason the core gave."), "a refusal must reach the reader rather than vanish: " + summary);
    return Task.CompletedTask;
}

static async Task ReopensProtectedBytesWithBothPasswordRolesAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var result = await facade.ReopenProtectedAsync(
        session.SessionId,
        "protected.pdf",
        [2],
        "open-pw",
        "permissions-pw");

    Assert(result.IsSuccess && result.Value!.DisplayName == "protected.pdf", "the protected copy must become the current session");
    Assert(core.LastOpenWithPasswords == ("open-pw", "permissions-pw"), "the reopen must retain both roles for later full rewrites");
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

static async Task<(FakeCore Core, PdfDocumentFacade Facade, DocumentSession Session)> OpenThreePagesAsync()
{
    var core = new FakeCore { PageCount = 3 };
    var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("three.pdf", [1]))).Value!;
    // Tell the pages apart by width: 100, 101, 102.
    core.LastDocument!.Widths(100, 101, 102);
    return (core, facade, session);
}

static IReadOnlyList<double> Widths(DocumentSession session) => [.. session.Pages.Select(page => page.WidthPt)];

static async Task MovesAPageAndHandsBackTheNewLayoutAsync()
{
    var (core, facade, session) = await OpenThreePagesAsync();
    using var _ = facade;

    var result = await facade.EditPagesAsync(session.SessionId, new PageEdit.Move(0, 2));

    Assert(result.IsSuccess, "moving a page should succeed");
    Assert(core.PageEdits.Single() == new PdfCoreEdit.MovePages(0, 1, 2), "one page, from 0, landing at 2");
    Assert(Widths(result.Value!).SequenceEqual([101, 102, 100]), "the session must describe the pages in their new order");
    Assert(core.RefreshPreviewCalls == 1, "rendering reads the preview, so it must be rebuilt to show the new order");
}

static async Task AppendsABlankPageAsync()
{
    var (core, facade, session) = await OpenThreePagesAsync();
    using var _ = facade;

    var result = await facade.EditPagesAsync(session.SessionId, new PageEdit.InsertBlank(session.PageCount));
    var blocked = await facade.OpenAsync(new DocumentSource("other.pdf", [2]));

    Assert(result.IsSuccess && result.Value!.PageCount == 4, "a blank page should be appended");
    Assert(core.PageEdits.Single() == new PdfCoreEdit.InsertBlankPage(3), "the end position must reach the core");
    Assert(Widths(result.Value!).SequenceEqual([100, 101, 102, 595]), "existing pages keep their order and the new page is A4");
    Assert(core.RefreshPreviewCalls == 1, "the inserted page must be visible in the preview");
    Assert(!blocked.IsSuccess && blocked.Error!.RequiresPendingEditDecision, "an inserted page is unsaved work");
}

static async Task InsertsABlankPageBeforeAChosenPageAsync()
{
    var (core, facade, session) = await OpenThreePagesAsync();
    using var _ = facade;

    var result = await facade.EditPagesAsync(session.SessionId, new PageEdit.InsertBlank(1));

    Assert(result.IsSuccess && result.Value!.PageCount == 4, "a blank page should be inserted before page two");
    Assert(core.PageEdits.Single() == new PdfCoreEdit.InsertBlankPage(1), "the chosen insertion position must reach the core");
    Assert(Widths(result.Value!).SequenceEqual([100, 595, 101, 102]), "the inserted page must precede the chosen page without reordering the others");
    Assert(core.RefreshPreviewCalls == 1, "the preview must reflect the inserted page");
}

static async Task InsertsALandscapeBlankPageAsync()
{
    var (core, facade, session) = await OpenThreePagesAsync();
    using var _ = facade;

    var result = await facade.EditPagesAsync(session.SessionId, new PageEdit.InsertBlank(1, PageOrientation.Landscape));

    Assert(result.IsSuccess && result.Value!.PageCount == 4, "a landscape page should be inserted");
    Assert(core.PageEdits.Single() == new PdfCoreEdit.InsertBlankPage(1, PageOrientation.Landscape),
        "the orientation and position must reach the core together");
    Assert(Widths(result.Value!).SequenceEqual([100, 842, 101, 102]), "the new A4 page should be wider than it is tall");
    Assert(result.Value!.Pages[1].HeightPt == 595, "the inserted page should have landscape A4 dimensions");
    Assert(core.RefreshPreviewCalls == 1, "the preview must include the landscape page");
}

static async Task RefusesBlankPageInsertionAsync()
{
    var (core, facade, session) = await OpenThreePagesAsync();
    using var _ = facade;
    core.LastDocument!.PageEditingAllowed = false;

    var result = await facade.EditPagesAsync(session.SessionId, new PageEdit.InsertBlank(session.PageCount));

    Assert(!result.IsSuccess && result.Error!.Message == "This document does not allow adding a blank page.",
        "the refusal must explain the page assembly restriction");
    Assert(core.RefreshPreviewCalls == 0 && core.PageEdits.Count == 0, "a refused insert must not change the preview");
}

static async Task RotatesAPageAsync()
{
    var (core, facade, session) = await OpenThreePagesAsync();
    using var _ = facade;

    var result = await facade.EditPagesAsync(session.SessionId, new PageEdit.Rotate(1, 90));

    Assert(result.IsSuccess, "rotating a page should succeed");
    Assert(core.PageEdits.Single() == new PdfCoreEdit.RotatePage(1, 90), "the turn should reach the core as given");
    Assert(result.Value!.Pages[1].WidthPt == 842, "a quarter turn puts the page on its side");
    Assert(core.RefreshPreviewCalls == 1, "a rotation is drawn by the PDF, so the preview must be rebuilt");
}

static async Task RemovesAPageAndKeepsTheCurrentPageInRangeAsync()
{
    var (_, facade, session) = await OpenThreePagesAsync();
    using var __ = facade;
    await facade.NavigateAsync(session.SessionId, 2);

    var result = await facade.EditPagesAsync(session.SessionId, new PageEdit.Remove(2));

    Assert(result.IsSuccess, "removing a page should succeed");
    Assert(result.Value!.PageCount == 2, "the page is gone");
    Assert(result.Value!.PageIndex == 1, "the current page cannot point past the end");
    Assert(Widths(result.Value!).SequenceEqual([100, 101]), "the other pages keep their order");
}

static async Task RefusesToRemoveTheOnlyPageAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("one.pdf", [1]))).Value!;

    var result = await facade.EditPagesAsync(session.SessionId, new PageEdit.Remove(0));

    Assert(!result.IsSuccess, "a document with no pages is not one the reader can do anything with");
    Assert(result.Error!.Message == "A document needs at least one page.", "the refusal should say why");
    Assert(core.PageEdits.Count == 0 && core.RefreshPreviewCalls == 0, "the core should not even be asked");
}

static async Task ExplainsADocumentThatForbidsPageChangesAsync()
{
    var (core, facade, session) = await OpenThreePagesAsync();
    using var _ = facade;
    core.LastDocument!.PageEditingAllowed = false;

    var result = await facade.EditPagesAsync(session.SessionId, new PageEdit.Move(0, 1));

    Assert(!result.IsSuccess, "a refused page edit must fail");
    Assert(result.Error!.Message == "This document does not allow its pages to be rearranged, rotated or removed.",
        "the generic 'not supported' says nothing the reader can act on");
    Assert(core.RefreshPreviewCalls == 0, "nothing was recorded, so nothing needs re-rendering");
}

static async Task CountsAPageEditAsUnsavedWorkAsync()
{
    var (_, facade, session) = await OpenThreePagesAsync();
    using var __ = facade;

    await facade.EditPagesAsync(session.SessionId, new PageEdit.Move(0, 2));
    var blocked = await facade.OpenAsync(new DocumentSource("other.pdf", [2]));

    Assert(!blocked.IsSuccess && blocked.Error!.RequiresPendingEditDecision, "a reordered document has work to lose");
}

static async Task RefreshesThePreviewOnHistoryAfterAPageEditAsync()
{
    var (core, facade, session) = await OpenThreePagesAsync();
    using var _ = facade;

    await facade.EditPagesAsync(session.SessionId, new PageEdit.Move(0, 2));
    await facade.UndoAsync(session.SessionId);

    Assert(core.RefreshPreviewCalls == 2, "undoing a move must put the pages back in the preview too");
}

static async Task ReportsTheCurrentLayoutAfterHistoryAsync()
{
    var (core, facade, session) = await OpenThreePagesAsync();
    using var _ = facade;
    await facade.EditPagesAsync(session.SessionId, new PageEdit.Remove(0));
    // What an undo does to the page model is the core's business; the fake
    // stands in for it by growing the page back.
    core.LastDocument!.Widths(100, 101, 102);

    var result = await facade.SessionAsync(session.SessionId);

    Assert(result.IsSuccess, "the current session should be readable");
    Assert(result.Value!.PageCount == 3 && Widths(result.Value!).SequenceEqual([100, 101, 102]),
        "the layout is read from the document now, not remembered from open");
}

static async Task RefusesPageEditsAfterSessionSwapAsync()
{
    var (core, facade, session) = await OpenThreePagesAsync();
    using var _ = facade;
    await facade.OpenAsync(new DocumentSource("second.pdf", [2]));

    var result = await facade.EditPagesAsync(session.SessionId, new PageEdit.Rotate(0, 90));

    Assert(!result.IsSuccess && result.Error!.Message == "The document is no longer available.",
        "a page position means nothing against another document");
    Assert(core.PageEdits.Count == 0, "the core should not be asked");
}

static void Assert(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}

static Task TracesFormPlacementInEitherDirection()
{
    var page = new PageDimensions(600, 800, PageRotation.None);
    var forward = FormPlacementRect.Resolve(new AnnotationPoint(100, 400), new AnnotationPoint(300, 450), page, 144, 36);
    var reverse = FormPlacementRect.Resolve(new AnnotationPoint(300, 450), new AnnotationPoint(100, 400), page, 144, 36);
    Assert(forward == new PdfCoreRect(100, 400, 200, 50) && reverse == forward,
        "a drag must preserve the traced geometry regardless of direction");
    return Task.CompletedTask;
}

static Task UsesClickDimensionsForTinyFormGesture()
{
    var page = new PageDimensions(600, 800, PageRotation.None);
    var rect = FormPlacementRect.Resolve(new AnnotationPoint(590, 10), new AnnotationPoint(593, 12), page, 18, 18);
    Assert(rect == new PdfCoreRect(582, 0, 18, 18), "a tiny motion should place a full checkbox inside the page");
    return Task.CompletedTask;
}

static Task ClampsFormPlacementOnRotatedPages()
{
    var page = new PageDimensions(800, 600, PageRotation.Clockwise90);
    var rect = FormPlacementRect.Resolve(new AnnotationPoint(590, 780), new AnnotationPoint(650, 850), page, 144, 36);
    Assert(rect == new PdfCoreRect(590, 780, 10, 20), "PDF coordinates must use the unrotated 600x800 bounds");
    return Task.CompletedTask;
}

static void AssertClose(double actual, double expected, string message)
{
    if (Math.Abs(actual - expected) > 1e-6)
    {
        throw new InvalidOperationException($"{message} (expected {expected}, got {actual})");
    }
}

static async Task ReadsPageContentForEditingAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    Assert(session.ContentEditingAllowed, "an unrestricted document should permit content editing");

    var result = await facade.PageContentAsync(session.SessionId, 0);

    Assert(result.IsSuccess, "page content should load");
    var run = result.Value!.TextRuns.Single();
    Assert(run.Text == "Hello world", "the run's text should reach the shell");
    Assert(run.IsEditable, "a standard-14 run is editable");
    Assert(run.Bounds.X == 100 && run.Bounds.Y == 700, "the run's PDF-space box should reach the shell");
    Assert(run.BaseFont == "Helvetica", "the run should be joined to the font its resource name points at");
}

static async Task RefusesPageContentWhenTheDocumentForbidsItAsync()
{
    var core = new FakeCore { ContentEditingPermitted = false };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("locked.pdf", [1]))).Value!;
    Assert(!session.ContentEditingAllowed, "the session should carry the document's refusal");

    var result = await facade.PageContentAsync(session.SessionId, 0);

    Assert(!result.IsSuccess, "content of a document that forbids editing should not be handed out");
    Assert(result.Error!.Message == "This document does not permit content changes.", "the refusal should be user safe");
    Assert(core.PageContentReads.IsEmpty, "the core should not even be asked");
}

static async Task ReplacesATextRunAndRefreshesThePreviewAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var run = (await facade.PageContentAsync(session.SessionId, 0)).Value!.TextRuns.Single();

    var result = await facade.ReplaceTextRunAsync(session.SessionId, run, "Goodbye world");

    Assert(result.IsSuccess, "retyping a standard-14 run should succeed");
    var edit = core.ContentEdits.Single();
    Assert(edit.After == "Goodbye world", "the typed text should reach the core");
    Assert(edit.Item.Id == run.Id && edit.Item.Text == "Hello world",
        "the core's own snapshot of the run must travel back unchanged — it is how the edit is re-found at save time");
    Assert(core.RefreshPreviewCalls == 1, "a content edit must rebuild the preview, or the page still shows the old words");
    Assert(result.Value!.CanUndo, "the edit should be undoable");
}

static async Task SubstitutesACompositeTextRunExplicitlyAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var run = ContentRun(new PdfCoreContentTextRun(
        7,
        0,
        new PdfCoreRect(100, 700, 80, 12),
        "F1",
        PdfCoreFontKind.EmbeddedComposite,
        "replacement characters"));

    var result = await facade.ReplaceTextRunWithInsertedFontAsync(session.SessionId, run, "Alberto Baro");

    Assert(result.IsSuccess, "an explicitly confirmed composite substitution should succeed");
    var edit = core.SubstitutionEdits.Single();
    Assert(edit.After == "Alberto Baro", "the replacement text should reach the dedicated core command");
    Assert(core.ContentEdits.Count == 0, "font-preserving replacement must not be selected implicitly");
}

static async Task NamesTheCharacterAFontCannotShowAsync()
{
    var core = new FakeCore { UnencodableCharacter = "\u4e16" };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var run = (await facade.PageContentAsync(session.SessionId, 0)).Value!.TextRuns.Single();

    var result = await facade.ReplaceTextRunAsync(session.SessionId, run, "Hello \u4e16");

    Assert(!result.IsSuccess, "a character the font cannot encode must be refused");
    Assert(result.Error!.Message.Contains('\u4e16'), "the reader typed the character; the message has to name it");
    Assert(core.RefreshPreviewCalls == 0, "nothing was recorded, so nothing needs re-rendering");
}

static async Task KeepsTheEditWhenThePreviewRefreshFailsAsync()
{
    var core = new FakeCore { RefreshPreviewThrows = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;
    var run = (await facade.PageContentAsync(session.SessionId, 0)).Value!.TextRuns.Single();

    var result = await facade.ReplaceTextRunAsync(session.SessionId, run, "Goodbye world");

    Assert(!result.IsSuccess, "the reader has to be told the preview did not catch up");
    Assert(core.ContentEdits.Count == 1, "the edit stays recorded: a stale preview is a better outcome than a silently dropped edit");
    var blocked = await facade.OpenAsync(new DocumentSource("other.pdf", [2]));
    Assert(!blocked.IsSuccess, "the document should still count as having unsaved work");
}

static async Task RefreshesThePreviewOnHistoryOnlyAfterAContentEditAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0, new PdfCoreRect(10, 10, 20, 20), new PdfCoreColor(255, 235, 0)));
    await facade.UndoAsync(session.SessionId);
    Assert(core.RefreshPreviewCalls == 0, "annotation history never changes what the PDF itself paints");

    var run = (await facade.PageContentAsync(session.SessionId, 0)).Value!.TextRuns.Single();
    await facade.ReplaceTextRunAsync(session.SessionId, run, "Goodbye world");
    Assert(core.RefreshPreviewCalls == 1, "the edit itself refreshes once");

    await facade.UndoAsync(session.SessionId);
    Assert(core.RefreshPreviewCalls == 2, "undoing a content edit must drop it from the preview too");
    await facade.RedoAsync(session.SessionId);
    Assert(core.RefreshPreviewCalls == 3, "and redoing it must bring it back");
}

static async Task ReadsEffectiveDocumentPropertiesAsync()
{
    var core = new FakeCore
    {
        DocumentInfo = new PdfCoreDocumentInfo("Vitela", "A. Editor", null, "pdf", "Creator", "Producer", new object(), new object()),
    };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var result = await facade.DocumentInfoAsync(session.SessionId);

    Assert(result.IsSuccess && result.Value!.Title == "Vitela", "the panel must read the core's effective metadata snapshot");
    Assert(result.Value!.Author == "A. Editor" && result.Value.Keywords == "pdf", "all exposed text fields must cross the facade");
}

static async Task UpdatesDocumentPropertiesWithoutDroppingDatesAsync()
{
    var creation = new object();
    var modified = new object();
    var core = new FakeCore
    {
        DocumentInfo = new PdfCoreDocumentInfo("Before", null, null, null, null, null, creation, modified),
    };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var result = await facade.SetDocumentInfoAsync(session.SessionId, new DocumentInfo("After", "Author", "", "keyword", "", "Producer"));

    Assert(result.IsSuccess && core.DocumentInfo.Title == "After", "the requested properties must be recorded");
    Assert(core.DocumentInfo.Subject is null && core.DocumentInfo.Creator is null, "empty fields must remove their Info keys");
    Assert(ReferenceEquals(core.DocumentInfo.CreationDate, creation) && ReferenceEquals(core.DocumentInfo.ModDate, modified), "dates outside this slice must survive unchanged");
    Assert(core.LastDocument!.CanUndo, "a metadata update must join shared history");
}

static async Task DoesNotRecordUnchangedDocumentPropertiesAsync()
{
    var core = new FakeCore
    {
        DocumentInfo = new PdfCoreDocumentInfo("Vitela", null, null, null, null, null, null, null),
    };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var result = await facade.SetDocumentInfoAsync(session.SessionId, new DocumentInfo("Vitela", "", "", "", "", ""));

    Assert(result.IsSuccess && !core.LastDocument!.CanUndo, "an unchanged snapshot must not add an undo step");
}

static async Task RefusesForbiddenDocumentPropertiesAsync()
{
    var core = new FakeCore { ContentEditingPermitted = false };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("sample.pdf", [1]))).Value!;

    var result = await facade.SetDocumentInfoAsync(session.SessionId, new DocumentInfo("Blocked", null, null, null, null, null));

    Assert(!result.IsSuccess, "metadata must use the general content-modification permission");
    Assert(!core.LastDocument!.CanUndo, "a refused edit must not enter history");
}

static List<PdfCoreFormField> SampleFormFields() =>
[
    new(0, 0, "name", new FormFieldKind.Text(Multiline: false, MaxLength: 20), new FormFieldValue.Text("")),
    new(1, 1, "agree", new FormFieldKind.Checkbox(), new FormFieldValue.Checked(false)),
    new(2, 1, "size", new FormFieldKind.Dropdown(["S", "M", "L"], Editable: false), new FormFieldValue.Choice(null)),
];

static async Task ListsFormFieldsAsync()
{
    var core = new FakeCore { FormFields = SampleFormFields() };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;

    var result = await facade.FormFieldsAsync(session.SessionId);

    Assert(result.IsSuccess && result.Value!.Fields.Count == 3, "every field the core reports must reach the panel");
    Assert(result.Value!.Fields[1] is { Name: "agree", PageIndex: 1, Kind: FormFieldKind.Checkbox }, "a field must keep its name, page and kind");
    Assert(result.Value.FillAllowed, "an unrestricted document permits filling");
    Assert(result.Value.StructureAllowed, "an unrestricted document permits structural edits");
}

static async Task FillsAFormFieldAsync()
{
    var core = new FakeCore { FormFields = SampleFormFields() };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;

    var result = await facade.SetFormFieldValueAsync(session.SessionId, 0, new FormFieldValue.Text("Ada"));

    Assert(result.IsSuccess && result.Value!.CanUndo, "a fill must join shared history");
    Assert(core.FormFields[0].Value == new FormFieldValue.Text("Ada"), "the core must record the typed value");
    Assert(core.RefreshPreviewCalls == 1, "pdfium paints field values, so the preview must be rebuilt to show it");
}

static async Task PlacesATextFieldAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
    var rect = new PdfCoreRect(90, 400, 144, 36);

    var result = await facade.AddTextFieldAsync(session.SessionId, 0, rect);

    Assert(result.IsSuccess && result.Value!.CanUndo, "creation must be one undoable edit");
    Assert(core.TextFieldEdits is [var field] && field.PageIndex == 0 && field.Rect == rect, "the click geometry must reach the core");
    Assert(core.FormFields.Count == 1 && core.RefreshPreviewCalls == 1, "the field must appear in the form list and the preview");
    await facade.UndoAsync(session.SessionId);
    Assert(core.RefreshPreviewCalls == 2, "undo must rebuild the preview after field creation");
}

static async Task RefusesForbiddenTextFieldPlacementAsync()
{
    foreach (var forbidContent in new[] { false, true })
    {
        var core = new FakeCore { ContentEditingPermitted = !forbidContent };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
        if (!forbidContent) core.LastDocument!.EditingAllowed = false;

        var result = await facade.AddTextFieldAsync(session.SessionId, 0, new PdfCoreRect(0, 0, 144, 36));
        Assert(!result.IsSuccess && result.Error!.Message == "This document does not permit creating form fields.", "permission refusal must be explicit");
        Assert(!core.LastDocument!.CanUndo && core.RefreshPreviewCalls == 0, "refused creation must not record an edit");
    }
}

static async Task PlacesACheckboxAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
    var rect = new PdfCoreRect(90, 400, 18, 18);

    var result = await facade.AddCheckboxAsync(session.SessionId, 0, rect);

    Assert(result.IsSuccess && result.Value!.CanUndo, "creation must be one undoable edit");
    Assert(core.CheckboxEdits is [var field] && field.PageIndex == 0 && field.Rect == rect, "the checkbox geometry must reach the core");
    Assert(core.FormFields is [{ Kind: FormFieldKind.Checkbox }] && core.RefreshPreviewCalls == 1, "the checkbox must appear in the form list and preview");
    await facade.UndoAsync(session.SessionId);
    Assert(core.RefreshPreviewCalls == 2, "undo must rebuild the preview after checkbox creation");
}

static async Task RefusesForbiddenCheckboxPlacementAsync()
{
    foreach (var forbidContent in new[] { false, true })
    {
        var core = new FakeCore { ContentEditingPermitted = !forbidContent };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
        if (!forbidContent) core.LastDocument!.EditingAllowed = false;

        var result = await facade.AddCheckboxAsync(session.SessionId, 0, new PdfCoreRect(0, 0, 18, 18));
        Assert(!result.IsSuccess && result.Error!.Message == "This document does not permit creating form fields.", "permission refusal must be explicit");
        Assert(!core.LastDocument!.CanUndo && core.RefreshPreviewCalls == 0, "refused creation must not record an edit");
    }
}

static async Task PlacesARadioGroupAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
    var rect = new PdfCoreRect(90, 400, 144, 48);

    var result = await facade.AddRadioGroupAsync(session.SessionId, 0, rect);

    Assert(result.IsSuccess && result.Value!.CanUndo, "creation must be one undoable edit");
    Assert(core.RadioGroupEdits is [var field] && field.PageIndex == 0 && field.Rect == rect, "the radio group geometry must reach the core");
    Assert(core.FormFields is [{ Kind: FormFieldKind.RadioGroup { Options: ["Option 1", "Option 2"] } }] && core.RefreshPreviewCalls == 1,
        "the new radio group must be visible for filling and in the preview");
    await facade.UndoAsync(session.SessionId);
    Assert(core.RefreshPreviewCalls == 2, "undo must rebuild the preview after radio group creation");
}

static async Task RefusesForbiddenRadioGroupPlacementAsync()
{
    foreach (var forbidContent in new[] { false, true })
    {
        var core = new FakeCore { ContentEditingPermitted = !forbidContent };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
        if (!forbidContent) core.LastDocument!.EditingAllowed = false;

        var result = await facade.AddRadioGroupAsync(session.SessionId, 0, new PdfCoreRect(0, 0, 144, 48));
        Assert(!result.IsSuccess && result.Error!.Message == "This document does not permit creating form fields.", "permission refusal must be explicit");
        Assert(!core.LastDocument!.CanUndo && core.RefreshPreviewCalls == 0, "refused creation must not record an edit");
    }
}

static async Task PlacesADropdownAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
    var rect = new PdfCoreRect(90, 400, 144, 36);

    var result = await facade.AddDropdownAsync(session.SessionId, 0, rect);

    Assert(result.IsSuccess && result.Value!.CanUndo, "creation must be one undoable edit");
    Assert(core.DropdownEdits is [var field] && field.PageIndex == 0 && field.Rect == rect, "the dropdown geometry must reach the core");
    Assert(core.FormFields is [{ Kind: FormFieldKind.Dropdown { Editable: false, Options: ["Option 1", "Option 2"] } }] && core.RefreshPreviewCalls == 1,
        "the dropdown must appear in the form list and preview with the default options");
    await facade.UndoAsync(session.SessionId);
    Assert(core.RefreshPreviewCalls == 2, "undo must rebuild the preview after dropdown creation");
}

static async Task RefusesForbiddenDropdownPlacementAsync()
{
    foreach (var forbidContent in new[] { false, true })
    {
        var core = new FakeCore { ContentEditingPermitted = !forbidContent };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
        if (!forbidContent) core.LastDocument!.EditingAllowed = false;

        var result = await facade.AddDropdownAsync(session.SessionId, 0, new PdfCoreRect(0, 0, 144, 36));
        Assert(!result.IsSuccess && result.Error!.Message == "This document does not permit creating form fields.", "permission refusal must be explicit");
        Assert(!core.LastDocument!.CanUndo && core.RefreshPreviewCalls == 0, "refused creation must not record an edit");
    }
}

static async Task DoesNotRecordAnUnchangedFillAsync()
{
    var core = new FakeCore { FormFields = SampleFormFields() };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;

    var result = await facade.SetFormFieldValueAsync(session.SessionId, 1, new FormFieldValue.Checked(false));

    Assert(result.IsSuccess && !core.LastDocument!.CanUndo, "leaving a field as it was must not add an undo step");
    Assert(core.RefreshPreviewCalls == 0, "and must not pay for a preview rebuild");
}

static async Task RefusesAForbiddenFillAsync()
{
    var core = new FakeCore { FormFields = SampleFormFields() };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
    core.LastDocument!.EditingAllowed = false;

    var state = (await facade.FormFieldsAsync(session.SessionId)).Value!;
    var result = await facade.SetFormFieldValueAsync(session.SessionId, 0, new FormFieldValue.Text("Ada"));

    Assert(!state.FillAllowed, "the panel must learn the refusal before the reader types");
    Assert(!result.IsSuccess && result.Error!.Message == "This document does not permit filling in its form.", "the refusal must be user safe");
    Assert(!core.LastDocument.CanUndo && core.RefreshPreviewCalls == 0, "a refused fill must not enter history or rebuild the preview");
}

static async Task FillsWhenOnlyContentEditingIsForbiddenAsync()
{
    // ISO 32000-1 table 22 bit 6 grants filling without bit 4: a shell that
    // reused the content-editing answer would lock a fillable form.
    var core = new FakeCore { FormFields = SampleFormFields(), ContentEditingPermitted = false };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;

    var result = await facade.SetFormFieldValueAsync(session.SessionId, 2, new FormFieldValue.Choice("M"));

    Assert(result.IsSuccess, "filling needs only the annotation/fill permission");
    Assert(core.FormFields[2].Value == new FormFieldValue.Choice("M"), "the choice must be recorded");
    Assert(!(await facade.FormFieldsAsync(session.SessionId)).Value!.StructureAllowed, "fill-only permission must not expose structural edits");
}

static async Task RenamesAFormFieldAsync()
{
    var core = new FakeCore { FormFields = SampleFormFields() };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;

    var result = await facade.RenameFormFieldAsync(session.SessionId, 1, "agree", "accepted");

    Assert(result.IsSuccess && result.Value!.CanUndo, "a rename must join shared history");
    Assert(core.FormFields[1].Name == "accepted" && core.RefreshPreviewCalls == 1, "the name must reach the core and preview");
    Assert((await facade.FormFieldsAsync(session.SessionId)).Value!.Fields[1].Name == "accepted", "the panel must see the new name");
    var unchanged = await facade.RenameFormFieldAsync(session.SessionId, 1, "accepted", "accepted");
    Assert(unchanged.IsSuccess && core.RefreshPreviewCalls == 1, "leaving a name unchanged must not rebuild the preview");
    await facade.UndoAsync(session.SessionId);
    Assert(core.RefreshPreviewCalls == 2, "undoing a structural rename must refresh the preview");
}

static async Task MovesAFormFieldAsync()
{
    var rect = new PdfCoreRect(30, 40, 120, 24);
    var core = new FakeCore { FormFields = [new PdfCoreFormField(7, 0, "name", new FormFieldKind.Text(false, null), new FormFieldValue.Text(""), Rect: rect)] };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
    var original = new AnnotationRect(30, 40, 120, 24);

    Assert((await facade.FormFieldsAsync(session.SessionId)).Value!.Fields[0].Rect == original, "the panel must receive the PDF-space rect");
    var moved = await facade.MoveFormFieldAsync(session.SessionId, 7, original, 50, 70);
    Assert(moved.IsSuccess && core.FormFields[0].Rect == rect with { X = 50, Y = 70 }, "move must preserve field dimensions");
    Assert(core.RefreshPreviewCalls == 1, "move must refresh the page preview");
    var unchanged = await facade.MoveFormFieldAsync(session.SessionId, 7, new AnnotationRect(50, 70, 120, 24), 50, 70);
    Assert(unchanged.IsSuccess && core.RefreshPreviewCalls == 1, "unchanged coordinates must not record another edit");
    var stale = await facade.MoveFormFieldAsync(session.SessionId, 7, original, 60, 80);
    Assert(!stale.IsSuccess && core.FormFields[0].Rect == rect with { X = 50, Y = 70 }, "a stale row must not move the field");
    var invalid = await facade.MoveFormFieldAsync(session.SessionId, 7, new AnnotationRect(50, 70, 120, 24), double.NaN, 80);
    Assert(!invalid.IsSuccess && core.RefreshPreviewCalls == 1, "non-finite coordinates must be rejected");
    var fillOnly = new FakeCore { ContentEditingPermitted = false, FormFields = [new PdfCoreFormField(7, 0, "name", new FormFieldKind.Text(false, null), new FormFieldValue.Text(""), Rect: rect)] };
    using var restricted = new PdfDocumentFacade(fillOnly, new RecordingLogger());
    var restrictedSession = (await restricted.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
    var forbidden = await restricted.MoveFormFieldAsync(restrictedSession.SessionId, 7, original, 60, 80);
    Assert(!forbidden.IsSuccess && fillOnly.RefreshPreviewCalls == 0, "filling permission alone cannot move a field");
}

static async Task ResizesAFormFieldAsync()
{
    var rect = new PdfCoreRect(30, 40, 120, 24);
    var field = new PdfCoreFormField(7, 0, "name", new FormFieldKind.Text(false, null), new FormFieldValue.Text("Ada"), Rect: rect);
    var core = new FakeCore { FormFields = [field] };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
    var original = new AnnotationRect(30, 40, 120, 24);
    var resized = new AnnotationRect(30, 40, 180, 36);

    var result = await facade.ResizeFormFieldAsync(session.SessionId, 7, original, 180, 36);
    Assert(result.IsSuccess && result.Value!.CanUndo, "resizing must join shared history");
    Assert(core.FormFields[0] == field with { Rect = rect with { Width = 180, Height = 36 } }, "resize must preserve origin and field definition");
    Assert(core.RefreshPreviewCalls == 1 && (await facade.FormFieldsAsync(session.SessionId)).Value!.Fields[0].Rect == resized,
        "the preview and panel must receive the resized field");
    var unchanged = await facade.ResizeFormFieldAsync(session.SessionId, 7, resized, 180, 36);
    Assert(unchanged.IsSuccess && core.RefreshPreviewCalls == 1, "unchanged dimensions must not record another edit");
    var stale = await facade.ResizeFormFieldAsync(session.SessionId, 7, original, 200, 40);
    Assert(!stale.IsSuccess, "a stale row must not overwrite a newer rectangle");
    foreach (var (width, height) in new[] { (double.NaN, 36.0), (180.0, double.PositiveInfinity), (0.0, 36.0), (180.0, -1.0) })
    {
        var invalid = await facade.ResizeFormFieldAsync(session.SessionId, 7, resized, width, height);
        Assert(!invalid.IsSuccess, "non-finite or non-positive dimensions must be rejected");
    }
    var missing = await facade.ResizeFormFieldAsync(session.SessionId, 99, resized, 200, 40);
    Assert(!missing.IsSuccess && core.RefreshPreviewCalls == 1, "refused edits must not refresh the preview");
    Assert(core.FormFields[0].Rect == rect with { Width = 180, Height = 36 }, "refused edits must preserve the field");
    await facade.UndoAsync(session.SessionId);
    Assert(core.RefreshPreviewCalls == 2, "undoing a structural resize must refresh the preview");

    foreach (var annotationAllowed in new[] { true, false })
    {
        var restrictedCore = new FakeCore { ContentEditingPermitted = !annotationAllowed, FormFields = [field] };
        using var restricted = new PdfDocumentFacade(restrictedCore, new RecordingLogger());
        var restrictedSession = (await restricted.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
        restrictedCore.LastDocument!.EditingAllowed = annotationAllowed;
        var forbidden = await restricted.ResizeFormFieldAsync(restrictedSession.SessionId, 7, original, 180, 36);
        Assert(!forbidden.IsSuccess && restrictedCore.FormFields[0] == field && restrictedCore.RefreshPreviewCalls == 0,
            "both structural permissions are required to resize a field");
    }
}

static async Task RestylesAFormFieldAsync()
{
    var style = new FormTextStyle(FormFont.Courier, 12, new AnnotationColor(20, 30, 40));
    var core = new FakeCore { FormFields = [new PdfCoreFormField(7, 0, "name", new FormFieldKind.Text(false, null), new FormFieldValue.Text("Ada"), style)] };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;

    var result = await facade.SetFormFieldFontSizeAsync(session.SessionId, 7, style, 16);
    Assert(result.IsSuccess && result.Value!.CanUndo, "restyling must join shared history");
    Assert(core.FormFields[0].Style == style with { SizePt = 16 } && core.RefreshPreviewCalls == 1,
        "the size must reach the core without replacing the font or color, and refresh the preview");
    Assert((await facade.FormFieldsAsync(session.SessionId)).Value!.Fields[0].Style?.SizePt == 16, "the panel must see the updated size");
    var unchanged = await facade.SetFormFieldFontSizeAsync(session.SessionId, 7, style with { SizePt = 16 }, 16);
    Assert(unchanged.IsSuccess && core.RefreshPreviewCalls == 1, "an unchanged size must not create a second edit");
    var stale = await facade.SetFormFieldFontSizeAsync(session.SessionId, 7, style, 18);
    Assert(!stale.IsSuccess && core.RefreshPreviewCalls == 1, "a stale row must not overwrite a more recent style");
    var invalid = await facade.SetFormFieldFontSizeAsync(session.SessionId, 7, style with { SizePt = 16 }, double.NaN);
    Assert(!invalid.IsSuccess && core.RefreshPreviewCalls == 1, "non-finite font sizes must not enter the edit log");
    var changedFont = await facade.SetFormFieldFontAsync(session.SessionId, 7, style with { SizePt = 16 }, FormFont.TimesRoman);
    Assert(changedFont.IsSuccess && core.FormFields[0].Style == style with { SizePt = 16, Font = FormFont.TimesRoman },
        "changing fonts must preserve the size and color");
    Assert(core.RefreshPreviewCalls == 2 && (await facade.FormFieldsAsync(session.SessionId)).Value!.Fields[0].Style?.Font == FormFont.TimesRoman,
        "the preview and panel must see the new font");
    var sameFont = await facade.SetFormFieldFontAsync(session.SessionId, 7, style with { SizePt = 16, Font = FormFont.TimesRoman }, FormFont.TimesRoman);
    Assert(sameFont.IsSuccess && core.RefreshPreviewCalls == 2, "choosing the current font must not create an edit");
    var staleFont = await facade.SetFormFieldFontAsync(session.SessionId, 7, style with { SizePt = 16 }, FormFont.Helvetica);
    Assert(!staleFont.IsSuccess && core.RefreshPreviewCalls == 2, "a stale font selection must not overwrite a newer style");
    var invalidFont = await facade.SetFormFieldFontAsync(session.SessionId, 7, style with { SizePt = 16, Font = FormFont.TimesRoman }, (FormFont)99);
    Assert(!invalidFont.IsSuccess && core.RefreshPreviewCalls == 2, "unknown fonts must not reach the core");
    var fontStyle = style with { SizePt = 16, Font = FormFont.TimesRoman };
    var color = new AnnotationColor(120, 80, 200);
    var changedColor = await facade.SetFormFieldColorAsync(session.SessionId, 7, fontStyle, color);
    Assert(changedColor.IsSuccess && core.FormFields[0].Style == fontStyle with { Color = color },
        "changing color must preserve the font and size");
    Assert(core.RefreshPreviewCalls == 3 && (await facade.FormFieldsAsync(session.SessionId)).Value!.Fields[0].Style?.Color == color,
        "the preview and panel must see the new color");
    var sameColor = await facade.SetFormFieldColorAsync(session.SessionId, 7, fontStyle with { Color = color }, color);
    Assert(sameColor.IsSuccess && core.RefreshPreviewCalls == 3, "closing the picker without changing color must not create an edit");
    var staleColor = await facade.SetFormFieldColorAsync(session.SessionId, 7, fontStyle, new AnnotationColor(0, 0, 0));
    Assert(!staleColor.IsSuccess && core.RefreshPreviewCalls == 3, "a stale color selection must not overwrite a newer style");
    core.LastDocument!.EditingAllowed = false;
    var forbidden = await facade.SetFormFieldFontSizeAsync(session.SessionId, 7, style with { SizePt = 16 }, 18);
    Assert(!forbidden.IsSuccess, "styling requires annotation permission");
    core.LastDocument.EditingAllowed = true;
    core.LastDocument!.ContentEditingAllowed = false;
    forbidden = await facade.SetFormFieldFontSizeAsync(session.SessionId, 7, style with { SizePt = 16 }, 18);
    Assert(!forbidden.IsSuccess, "fill-only permissions must not allow styling");
    forbidden = await facade.SetFormFieldFontAsync(session.SessionId, 7, style with { SizePt = 16, Font = FormFont.TimesRoman }, FormFont.Helvetica);
    Assert(!forbidden.IsSuccess && core.RefreshPreviewCalls == 3, "fill-only permissions must not allow font changes");
    forbidden = await facade.SetFormFieldColorAsync(session.SessionId, 7, fontStyle with { Color = color }, style.Color);
    Assert(!forbidden.IsSuccess && core.RefreshPreviewCalls == 3, "fill-only permissions must not allow color changes");
    core.LastDocument.ContentEditingAllowed = true;
    await facade.UndoAsync(session.SessionId);
    Assert(core.RefreshPreviewCalls == 4, "undoing a color change must refresh the preview");
}

static async Task RefusesInvalidFormRenameAsync()
{
    var core = new FakeCore { FormFields = SampleFormFields() };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;

    var stale = await facade.RenameFormFieldAsync(session.SessionId, 1, "old name", "accepted");
    Assert(!stale.IsSuccess && !core.LastDocument!.CanUndo, "a row with a stale name must not overwrite another edit");
    var missing = await facade.RenameFormFieldAsync(session.SessionId, 99, "missing", "accepted");
    Assert(!missing.IsSuccess, "a missing field must be refused");
    var document = core.LastDocument!;
    document.EditingAllowed = false;
    var forbidden = await facade.RenameFormFieldAsync(session.SessionId, 1, "agree", "accepted");
    Assert(!forbidden.IsSuccess && core.RefreshPreviewCalls == 0, "renaming requires the annotation permission");
    document.EditingAllowed = true;
    document.ContentEditingAllowed = false;
    forbidden = await facade.RenameFormFieldAsync(session.SessionId, 1, "agree", "accepted");
    Assert(!forbidden.IsSuccess && core.FormFields[1].Name == "agree", "renaming also requires content editing");
}

/// <summary>
/// The document the core's <c>form_field_editing_allowed</c> exists for: both
/// <c>/P</c> bits granted, but encrypted so a full rewrite is refused — and
/// with it content editing. A form edit never rewrites the file, so none of
/// the structural gates may borrow that refusal.
/// </summary>
static async Task EditsFormFieldsWhenOnlyAFullRewriteIsRefusedAsync()
{
    var rect = new PdfCoreRect(30, 40, 120, 24);
    var style = new FormTextStyle(FormFont.Courier, 12, new AnnotationColor(20, 30, 40));
    var core = new FakeCore { FormFields = [new PdfCoreFormField(7, 0, "name", new FormFieldKind.Text(false, null), new FormFieldValue.Text(""), style, rect)] };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
    var document = core.LastDocument!;
    document.FullRewriteAllowed = false;
    Assert(!core.ContentEditingAllowed(document) && core.FormFieldEditingAllowed(document),
        "precondition: content editing refused, form-field editing allowed");

    Assert((await facade.FormFieldsAsync(session.SessionId)).Value!.StructureAllowed, "the panel must offer structural edits");
    var moved = await facade.MoveFormFieldAsync(session.SessionId, 7, new AnnotationRect(30, 40, 120, 24), 50, 70);
    Assert(moved.IsSuccess, "moving must not borrow the full-rewrite refusal");
    var resized = await facade.ResizeFormFieldAsync(session.SessionId, 7, new AnnotationRect(50, 70, 120, 24), 180, 36);
    Assert(resized.IsSuccess, "resizing must not borrow the full-rewrite refusal");
    var renamed = await facade.RenameFormFieldAsync(session.SessionId, 7, "name", "full_name");
    Assert(renamed.IsSuccess, "renaming must not borrow the full-rewrite refusal");
    var restyled = await facade.SetFormFieldFontSizeAsync(session.SessionId, 7, style, 16);
    Assert(restyled.IsSuccess, "restyling must not borrow the full-rewrite refusal");
    var created = await facade.AddTextFieldAsync(session.SessionId, 0, new PdfCoreRect(0, 0, 144, 36));
    Assert(created.IsSuccess && core.FormFields.Count == 2, "creating must not borrow the full-rewrite refusal");
}

static async Task InsertsContentImageAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
    var target = (await facade.PrepareImageInsertionAsync(session.SessionId, 0)).Value!;
    byte[] bytes = [1, 2, 3];
    var result = await facade.InsertContentImageAsync(session.SessionId, target, bytes, -20, 0);
    Assert(result.IsSuccess && result.Value!.CanUndo && core.RefreshPreviewCalls == 1, "insertion must record and refresh");
    var edit = core.ImageInsertions.Single();
    Assert(edit.Item.PageIndex == 0 && edit.Item.Bbox == new PdfCoreRect(-20, -77, 77, 77), "the core's top-left placement must be used unchanged");
    bytes[0] = 9;
    Assert(edit.Source.SequenceEqual(new byte[] { 1, 2, 3 }), "the edit must own its source for later save and redo");
    Assert(!(await facade.InsertContentImageAsync(session.SessionId, target, bytes, 0, 0)).IsSuccess, "a submitted target must be stale");
    var next = (await facade.PrepareImageInsertionAsync(session.SessionId, 0)).Value!;
    Assert((await facade.InsertContentImageAsync(session.SessionId, next, bytes, 0, 0)).IsSuccess, "multiple pending images must work");
    Assert(core.ImageInsertions.Select(item => item.Item.ResourceXObjectName).Distinct().Count() == 2, "insertions need independent resources");
    Assert((await facade.UndoAsync(session.SessionId)).Value!.CanRedo, "insertion must be undoable");
    Assert(!(await facade.InsertContentImageAsync(session.SessionId, next, bytes, 0, 0)).IsSuccess, "undo must not revive a target");
    Assert((await facade.RedoAsync(session.SessionId)).Value!.CanUndo && core.RefreshPreviewCalls == 4, "redo must refresh");
}

static async Task ReplacesContentImagesAsync()
{
    foreach (var name in new string?[] { "Im1", null })
    {
        var source = new PdfCoreContentImage(8, 0, new PdfCoreRect(20, 30, 100, 50), name);
        var core = new FakeCore { PageImages = [source] };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
        var image = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
        Assert((await facade.PrepareImageReplacementAsync(session.SessionId, image)).IsSuccess && !core.LastDocument!.CanUndo, "preparation must leave history untouched");
        byte[] bytes = [4, 5, 6];
        var result = await facade.ReplaceContentImageAsync(session.SessionId, image, bytes);
        bytes[0] = 9;
        var edit = core.ImageReplacements.Single();
        Assert(result.IsSuccess && result.Value!.CanUndo && core.RefreshPreviewCalls == 1, "replacement must record and refresh");
        Assert(edit.Item == source && edit.Before.SequenceEqual(new byte[] { 1, 2, 3 }) && edit.After.SequenceEqual(new byte[] { 4, 5, 6 }), "source identity, geometry, undo bytes and owned replacement must be retained");
        Assert(!(await facade.ReplaceContentImageAsync(session.SessionId, image, [7])).IsSuccess, "submission must invalidate its snapshot");
        Assert((await facade.UndoAsync(session.SessionId)).Value!.CanRedo, "replacement must be undoable");
        Assert(!(await facade.PrepareImageReplacementAsync(session.SessionId, image)).IsSuccess, "undo must not revive snapshots");
        Assert((await facade.RedoAsync(session.SessionId)).Value!.CanUndo && core.RefreshPreviewCalls == 3, "redo must refresh");
    }
}

static async Task RefusesImageReplacementAsync()
{
    var core = new FakeCore { PageImages = [new(8, 0, new(20, 30, 100, 50), "Im1")] };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
    var image = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
    core.ImageSourceError = PdfCoreError.UnsupportedOperation;
    Assert(!(await facade.PrepareImageReplacementAsync(session.SessionId, image)).IsSuccess, "unrecoverable sources must refuse before the picker");
    Assert(!(await facade.ReplaceContentImageAsync(session.SessionId, image, [4])).IsSuccess, "readback must be checked again on submission");
    core.ImageSourceError = null;
    core.ImageReplacementError = PdfCoreError.InvalidImage;
    Assert(!(await facade.ReplaceContentImageAsync(session.SessionId, image, [])).IsSuccess, "invalid replacement must never enter history");
    core.ImageReplacementError = null;
    core.LastDocument!.ContentEditingAllowed = false;
    Assert(!(await facade.PrepareImageReplacementAsync(session.SessionId, image)).IsSuccess, "preparation must check permissions");
    Assert(!(await facade.ReplaceContentImageAsync(session.SessionId, image, [4])).IsSuccess, "submission must check permissions");
    core.LastDocument.ContentEditingAllowed = true;
    core.LastDocument.FullRewriteAllowed = false;
    Assert(!(await facade.ReplaceContentImageAsync(session.SessionId, image, [4])).IsSuccess, "unwritable edits must refuse");
    core.LastDocument.FullRewriteAllowed = true;
    await facade.MoveImageAsync(session.SessionId, image, 40, 60);
    Assert(!(await facade.PrepareImageReplacementAsync(session.SessionId, image)).IsSuccess, "intervening edits invalidate selection");
    var fresh = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
    var other = (await facade.OpenAsync(new DocumentSource("other.pdf", [2]), discardPendingEdits: true)).Value!;
    Assert(!(await facade.ReplaceContentImageAsync(other.SessionId, fresh, [4])).IsSuccess, "cross-session selection must refuse");
    Assert(!(await facade.ReplaceContentImageAsync(session.SessionId, fresh, [4])).IsSuccess, "retired sessions must refuse");
    Assert(core.ImageReplacements.Count == 0, "refusals must not record replacements");
}

static async Task KeepsImageReplacementAfterPreviewFailureAsync()
{
    var core = new FakeCore { PageImages = [new(8, 0, new(20, 30, 100, 50), null)], RefreshPreviewThrows = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
    var image = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
    var result = await facade.ReplaceContentImageAsync(session.SessionId, image, [4]);
    Assert(!result.IsSuccess && core.LastDocument!.CanUndo && core.ImageReplacements.Count == 1, "preview failure must retain the undoable replacement");
}

static async Task RefusesInvalidImageInsertionAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
    Assert(!(await facade.PrepareImageInsertionAsync(session.SessionId, session.PageCount)).IsSuccess, "missing pages must be refused");
    var target = (await facade.PrepareImageInsertionAsync(session.SessionId, 0)).Value!;
    foreach (var value in new[] { double.NaN, double.PositiveInfinity, double.NegativeInfinity })
    {
        Assert(!(await facade.InsertContentImageAsync(session.SessionId, target, [1], value, 0)).IsSuccess, "nonfinite X must be refused");
        Assert(!(await facade.InsertContentImageAsync(session.SessionId, target, [1], 0, value)).IsSuccess, "nonfinite Y must be refused");
    }
    core.StampPlacementError = PdfCoreError.InvalidImage;
    Assert(!(await facade.InsertContentImageAsync(session.SessionId, target, [], 0, 0)).IsSuccess, "undecodable bytes must not record");
    core.StampPlacementError = null;
    core.LastDocument!.ContentEditingAllowed = false;
    Assert(!(await facade.PrepareImageInsertionAsync(session.SessionId, 0)).IsSuccess, "preparation must check permission");
    Assert(!(await facade.InsertContentImageAsync(session.SessionId, target, [1], 0, 0)).IsSuccess, "submission must recheck permission");
    core.LastDocument.ContentEditingAllowed = true;
    core.LastDocument.FullRewriteAllowed = false;
    Assert(!(await facade.InsertContentImageAsync(session.SessionId, target, [1], 0, 0)).IsSuccess, "a full rewrite must be possible");
    core.LastDocument.FullRewriteAllowed = true;
    await facade.ReplaceTextRunAsync(session.SessionId, (await facade.PageContentAsync(session.SessionId, 0)).Value!.TextRuns[0], "Changed");
    Assert(!(await facade.InsertContentImageAsync(session.SessionId, target, [1], 0, 0)).IsSuccess, "intervening edits must invalidate the target");
    var fresh = (await facade.PrepareImageInsertionAsync(session.SessionId, 0)).Value!;
    var other = (await facade.OpenAsync(new DocumentSource("other.pdf", [2]), discardPendingEdits: true)).Value!;
    Assert(!(await facade.InsertContentImageAsync(other.SessionId, fresh, [1], 0, 0)).IsSuccess, "cross-session targets must be refused");
    Assert(!(await facade.InsertContentImageAsync(session.SessionId, fresh, [1], 0, 0)).IsSuccess, "retired sessions must be refused");
    Assert(core.ImageInsertions.Count == 0, "refusals must not record images");
}

static async Task KeepsImageInsertionAfterPreviewFailureAsync()
{
    var core = new FakeCore { RefreshPreviewThrows = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
    var target = (await facade.PrepareImageInsertionAsync(session.SessionId, 0)).Value!;
    var result = await facade.InsertContentImageAsync(session.SessionId, target, [1], 0, 0);
    Assert(!result.IsSuccess && core.LastDocument!.CanUndo && core.ImageInsertions.Count == 1, "preview failure must retain the undoable edit");
}

static async Task InsertsContentTextAsync()
{
    var core = new FakeCore { FontFamilies = new Dictionary<string, string> { ["F1"] = "Courier", ["VitelaText1"] = "Times-Roman" } };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("text.pdf", [1]))).Value!;
    var target = (await facade.PrepareTextInsertionAsync(session.SessionId, 0)).Value!;
    var inserted = await facade.InsertTextRunAsync(session.SessionId, target, "New text", -20, 0, 14);
    Assert(inserted.IsSuccess && inserted.Value!.CanUndo && core.RefreshPreviewCalls == 1, "insertion must record and refresh");
    var item = core.TextInsertions.Single().Item;
    Assert(item.Text == "New text" && item.PageIndex == 0 && item.Bbox == new PdfCoreRect(-20, 0, 200, 14), "text and PDF-space geometry must reach the core");
    Assert(!core.FontFamilies.ContainsKey(item.ResourceFontName), "existing resources, including unused fonts, must not be reused");
    Assert(!(await facade.InsertTextRunAsync(session.SessionId, target, "Duplicate", 0, 0, 14)).IsSuccess, "a submitted dialog must become stale");
    var next = (await facade.PrepareTextInsertionAsync(session.SessionId, 0)).Value!;
    Assert((await facade.InsertTextRunAsync(session.SessionId, next, "Second", 0, 0, 72)).IsSuccess, "multiple pending insertions must work");
    Assert(core.TextInsertions.Select(edit => edit.Item.ResourceFontName).Distinct().Count() == 2, "pending insertions need independent fonts");
    Assert((await facade.UndoAsync(session.SessionId)).Value!.CanRedo, "insertion must be undoable");
    Assert(!(await facade.InsertTextRunAsync(session.SessionId, next, "Stale", 0, 0, 14)).IsSuccess, "undo must not revive a dialog");
    Assert((await facade.RedoAsync(session.SessionId)).Value!.CanUndo && core.RefreshPreviewCalls == 4, "redo must refresh");
}

static async Task RefusesInvalidTextInsertionAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("text.pdf", [1]))).Value!;
    Assert(!(await facade.PrepareTextInsertionAsync(session.SessionId, session.PageCount)).IsSuccess, "missing pages must be refused");
    var target = (await facade.PrepareTextInsertionAsync(session.SessionId, 0)).Value!;
    foreach (var text in new[] { "", " ", "a\nb", "a\rb" })
        Assert(!(await facade.InsertTextRunAsync(session.SessionId, target, text, 0, 0, 14)).IsSuccess, "empty or multiline text must not record");
    foreach (var value in new[] { double.NaN, double.PositiveInfinity, double.NegativeInfinity })
    {
        Assert(!(await facade.InsertTextRunAsync(session.SessionId, target, "Text", value, 0, 14)).IsSuccess, "nonfinite X must be refused");
        Assert(!(await facade.InsertTextRunAsync(session.SessionId, target, "Text", 0, value, 14)).IsSuccess, "nonfinite Y must be refused");
    }
    foreach (var size in new[] { double.NaN, double.PositiveInfinity, 0, -1, 0.5, 73 })
        Assert(!(await facade.InsertTextRunAsync(session.SessionId, target, "Text", 0, 0, size)).IsSuccess, "invalid size must be refused");
    core.LastDocument!.ContentEditingAllowed = false;
    Assert(!(await facade.PrepareTextInsertionAsync(session.SessionId, 0)).IsSuccess, "preparation must check permissions");
    Assert(!(await facade.InsertTextRunAsync(session.SessionId, target, "Text", 0, 0, 14)).IsSuccess, "submission must recheck permissions");
    core.LastDocument.ContentEditingAllowed = true;
    core.LastDocument.FullRewriteAllowed = false;
    Assert(!(await facade.InsertTextRunAsync(session.SessionId, target, "Text", 0, 0, 14)).IsSuccess, "a full rewrite must be possible");
    core.LastDocument.FullRewriteAllowed = true;
    await facade.ReplaceTextRunAsync(session.SessionId, (await facade.PageContentAsync(session.SessionId, 0)).Value!.TextRuns[0], "Changed");
    Assert(!(await facade.InsertTextRunAsync(session.SessionId, target, "Text", 0, 0, 14)).IsSuccess, "intervening edits must invalidate the target");
    var fresh = (await facade.PrepareTextInsertionAsync(session.SessionId, 0)).Value!;
    var other = (await facade.OpenAsync(new DocumentSource("other.pdf", [2]), discardPendingEdits: true)).Value!;
    Assert(!(await facade.InsertTextRunAsync(other.SessionId, fresh, "Text", 0, 0, 14)).IsSuccess, "cross-session targets must be refused");
    Assert(!(await facade.InsertTextRunAsync(session.SessionId, fresh, "Text", 0, 0, 14)).IsSuccess, "retired sessions must be refused");
    Assert(core.TextInsertions.Count == 0, "refusals must not record insertions");
}

static async Task KeepsTextInsertionAfterPreviewFailureAsync()
{
    var core = new FakeCore { RefreshPreviewThrows = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("text.pdf", [1]))).Value!;
    var target = (await facade.PrepareTextInsertionAsync(session.SessionId, 0)).Value!;
    var result = await facade.InsertTextRunAsync(session.SessionId, target, "Text", 0, 0, 14);
    Assert(!result.IsSuccess && core.LastDocument!.CanUndo && core.TextInsertions.Count == 1, "a preview failure must retain undoable insertion");
}

static async Task DeletesContentTextAsync()
{
    foreach (var kind in Enum.GetValues<PdfCoreFontKind>())
    {
        var source = new PdfCoreContentTextRun(7, 0, new PdfCoreRect(100, 700, 120, 12), "F1", kind, "Hello world");
        var core = new FakeCore { PageTextRuns = [source] };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("text.pdf", [1]))).Value!;
        var run = (await facade.PageTextEditTargetsAsync(session.SessionId, 0)).Value![0];
        var deleted = await facade.RemoveTextRunAsync(session.SessionId, run);
        Assert(deleted.IsSuccess && deleted.Value!.CanUndo && core.RefreshPreviewCalls == 1, "deletion must refresh and enter history");
        Assert(core.TextRemovals.Single().Item == source && core.SubstitutionEdits.Count == 0, "deletion must preserve the original run without substituting its font");
        Assert(!(await facade.RemoveTextRunAsync(session.SessionId, run)).IsSuccess, "a deleted snapshot must not record a second removal");
        Assert((await facade.UndoAsync(session.SessionId)).Value!.CanRedo, "deletion must be undoable");
        Assert(!(await facade.RemoveTextRunAsync(session.SessionId, run)).IsSuccess, "undo must not revive a snapshot");
        Assert((await facade.RedoAsync(session.SessionId)).Value!.CanUndo && core.RefreshPreviewCalls == 3, "redo must refresh the preview");
    }
}

static async Task MovesContentTextAsync()
{
    foreach (var kind in Enum.GetValues<PdfCoreFontKind>())
    {
        var source = new PdfCoreContentTextRun(7, 0, new PdfCoreRect(100, 700, 120, 12), "F1", kind, "Hello world");
        var core = new FakeCore { PageTextRuns = [source] };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("text.pdf", [1]))).Value!;
        var run = (await facade.PageTextEditTargetsAsync(session.SessionId, 0)).Value![0];
        Assert((await facade.MoveTextRunAsync(session.SessionId, run, 100, 700)).IsSuccess
            && core.TextMoves.Count == 0 && core.RefreshPreviewCalls == 0, "unchanged coordinates must not enter history");
        var moved = await facade.MoveTextRunAsync(session.SessionId, run, -20, 0);
        Assert(moved.IsSuccess && moved.Value!.CanUndo && core.RefreshPreviewCalls == 1, "moving must record and refresh");
        Assert(core.TextMoves.Single().Item == source && core.TextMoves.Single().To == source.Bbox with { X = -20, Y = 0 }
            && core.SubstitutionEdits.Count == 0, "moving must preserve text, font, dimensions and source identity");
        Assert(!(await facade.MoveTextRunAsync(session.SessionId, run, 10, 20)).IsSuccess, "a used target must become stale");
        Assert((await facade.UndoAsync(session.SessionId)).Value!.CanRedo, "movement must be undoable");
        Assert(!(await facade.MoveTextRunAsync(session.SessionId, run, 10, 20)).IsSuccess, "undo must not revive a target");
        Assert((await facade.RedoAsync(session.SessionId)).Value!.CanUndo && core.RefreshPreviewCalls == 3, "redo must refresh");
    }
}

static async Task RefusesInvalidTextMovementAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("text.pdf", [1]))).Value!;
    var run = (await facade.PageTextEditTargetsAsync(session.SessionId, 0)).Value![0];
    foreach (var value in new[] { double.NaN, double.PositiveInfinity, double.NegativeInfinity })
    {
        Assert(!(await facade.MoveTextRunAsync(session.SessionId, run, value, 0)).IsSuccess, "nonfinite X must be refused");
        Assert(!(await facade.MoveTextRunAsync(session.SessionId, run, 0, value)).IsSuccess, "nonfinite Y must be refused");
    }
    var inline = (await facade.PageContentAsync(session.SessionId, 0)).Value!.TextRuns[0];
    Assert(!(await facade.MoveTextRunAsync(session.SessionId, inline, 0, 0)).IsSuccess, "unbound editor targets must be refused");
    core.LastDocument!.ContentEditingAllowed = false;
    Assert(!(await facade.MoveTextRunAsync(session.SessionId, run, 0, 0)).IsSuccess, "submission must recheck permissions");
    core.LastDocument.ContentEditingAllowed = true;
    core.LastDocument.FullRewriteAllowed = false;
    Assert(!(await facade.MoveTextRunAsync(session.SessionId, run, 0, 0)).IsSuccess, "unwritable documents must be refused");
    core.LastDocument.FullRewriteAllowed = true;
    await facade.ReplaceTextRunAsync(session.SessionId, inline, "Changed");
    Assert(!(await facade.MoveTextRunAsync(session.SessionId, run, 0, 0)).IsSuccess, "retyping must invalidate movement targets");
    var fresh = (await facade.PageTextEditTargetsAsync(session.SessionId, 0)).Value![0];
    var other = (await facade.OpenAsync(new DocumentSource("other.pdf", [2]), discardPendingEdits: true)).Value!;
    Assert(!(await facade.MoveTextRunAsync(other.SessionId, fresh, 0, 0)).IsSuccess, "another session must reject the target");
    Assert(!(await facade.MoveTextRunAsync(session.SessionId, fresh, 0, 0)).IsSuccess, "retired sessions must reject movement");
    Assert(core.TextMoves.Count == 0, "refusals must not record movement");
}

static async Task KeepsTextMovementAfterPreviewFailureAsync()
{
    var core = new FakeCore { RefreshPreviewThrows = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("text.pdf", [1]))).Value!;
    var run = (await facade.PageTextEditTargetsAsync(session.SessionId, 0)).Value![0];
    var result = await facade.MoveTextRunAsync(session.SessionId, run, 10, 20);
    Assert(!result.IsSuccess && core.LastDocument!.CanUndo && core.TextMoves.Count == 1, "preview failure must retain undoable movement");
}

static async Task RefusesInvalidTextDeletionAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("text.pdf", [1]))).Value!;
    var run = (await facade.PageTextEditTargetsAsync(session.SessionId, 0)).Value![0];
    var inlineRun = (await facade.PageContentAsync(session.SessionId, 0)).Value!.TextRuns[0];
    Assert(!(await facade.RemoveTextRunAsync(session.SessionId, inlineRun)).IsSuccess, "an inline-editor run is not a revision-bound deletion target");
    core.LastDocument!.ContentEditingAllowed = false;
    Assert(!(await facade.PageTextEditTargetsAsync(session.SessionId, 0)).IsSuccess, "reading deletion targets must check permission");
    Assert(!(await facade.RemoveTextRunAsync(session.SessionId, run)).IsSuccess, "submission must check permission again");
    core.LastDocument.ContentEditingAllowed = true;
    core.LastDocument.FullRewriteAllowed = false;
    Assert(!(await facade.RemoveTextRunAsync(session.SessionId, run)).IsSuccess, "an unwritable deletion must be refused");
    core.LastDocument.FullRewriteAllowed = true;
    Assert(core.TextRemovals.Count == 0 && core.RefreshPreviewCalls == 0, "refusals must not record or refresh");
    await facade.ReplaceTextRunAsync(session.SessionId, inlineRun, "Updated text");
    Assert(!(await facade.RemoveTextRunAsync(session.SessionId, run)).IsSuccess, "intervening retyping must invalidate the deletion target");
    var fresh = (await facade.PageTextEditTargetsAsync(session.SessionId, 0)).Value![0];
    var other = (await facade.OpenAsync(new DocumentSource("other.pdf", [2]), discardPendingEdits: true)).Value!;
    Assert(!(await facade.RemoveTextRunAsync(other.SessionId, fresh)).IsSuccess, "another session must not accept the same run id");
    Assert(!(await facade.RemoveTextRunAsync(session.SessionId, fresh)).IsSuccess, "a retired session must be refused");
    Assert(core.TextRemovals.Count == 0, "stale snapshots must not record deletions");
}

static async Task KeepsTextDeletionAfterPreviewFailureAsync()
{
    var core = new FakeCore { RefreshPreviewThrows = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("text.pdf", [1]))).Value!;
    var run = (await facade.PageTextEditTargetsAsync(session.SessionId, 0)).Value![0];
    var result = await facade.RemoveTextRunAsync(session.SessionId, run);
    Assert(!result.IsSuccess && core.LastDocument!.CanUndo && core.TextRemovals.Count == 1, "preview failure must retain the undoable deletion");
}

static async Task DeletesContentImagesAsync()
{
    foreach (var resource in new string?[] { "Im1", null })
    {
        var source = new PdfCoreContentImage(8, 0, new PdfCoreRect(20, 30, 100, 50), resource);
        var core = new FakeCore { PageImages = [source] };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
        var image = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
        var deleted = await facade.RemoveImageAsync(session.SessionId, image);
        Assert(deleted.IsSuccess && deleted.Value!.CanUndo && core.RefreshPreviewCalls == 1, "deletion must refresh and enter shared history");
        Assert(core.ImageRemovals.Single().Item == source, "deletion must preserve original image identity and resource/inline source");
        Assert(!(await facade.RemoveImageAsync(session.SessionId, image)).IsSuccess, "the deleted snapshot must not record a second removal");
        var undone = await facade.UndoAsync(session.SessionId);
        Assert(undone.IsSuccess && undone.Value!.CanRedo, "deletion must be undoable");
        Assert(!(await facade.RemoveImageAsync(session.SessionId, image)).IsSuccess, "undo must not revive a snapshot");
        var redone = await facade.RedoAsync(session.SessionId);
        Assert(redone.IsSuccess && redone.Value!.CanUndo && core.RefreshPreviewCalls == 3, "redo must rebuild the painted PDF");
    }
}

static async Task RefusesInvalidImageDeletionAsync()
{
    var core = new FakeCore { PageImages = [new PdfCoreContentImage(8, 0, new PdfCoreRect(20, 30, 100, 50), "Im1")] };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
    var image = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
    core.LastDocument!.ContentEditingAllowed = false;
    Assert(!(await facade.RemoveImageAsync(session.SessionId, image)).IsSuccess, "deletion must check content permission at submission");
    core.LastDocument.ContentEditingAllowed = true;
    core.LastDocument.FullRewriteAllowed = false;
    Assert(!(await facade.RemoveImageAsync(session.SessionId, image)).IsSuccess, "an unwritable content edit must be refused");
    core.LastDocument.FullRewriteAllowed = true;
    Assert(core.ImageRemovals.Count == 0 && core.RefreshPreviewCalls == 0, "refusals must not record or refresh");
    await facade.MoveImageAsync(session.SessionId, image, 40, 60);
    Assert(!(await facade.RemoveImageAsync(session.SessionId, image)).IsSuccess, "an intervening edit must invalidate deletion snapshots");
    var fresh = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
    var other = (await facade.OpenAsync(new DocumentSource("other.pdf", [2]), discardPendingEdits: true)).Value!;
    Assert(!(await facade.RemoveImageAsync(other.SessionId, fresh)).IsSuccess, "another session must not accept the same image id");
    Assert(!(await facade.RemoveImageAsync(session.SessionId, fresh)).IsSuccess, "a retired session must be refused");
    Assert(core.ImageRemovals.Count == 0, "stale snapshots must never delete an image");
}

static async Task KeepsImageDeletionAfterPreviewFailureAsync()
{
    var core = new FakeCore { PageImages = [new PdfCoreContentImage(8, 0, new PdfCoreRect(20, 30, 100, 50), null)], RefreshPreviewThrows = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
    var image = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
    var result = await facade.RemoveImageAsync(session.SessionId, image);
    Assert(!result.IsSuccess && core.LastDocument!.CanUndo && core.ImageRemovals.Count == 1, "a preview failure must retain the undoable deletion");
}

static async Task MovesContentImagesAsync()
{
    foreach (var resource in new string?[] { "Im1", null })
    {
        var source = new PdfCoreContentImage(8, 0, new PdfCoreRect(20, 30, 100, 50), resource);
        var core = new FakeCore { PageImages = [source] };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
        var image = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
        var unchanged = await facade.MoveImageAsync(session.SessionId, image, 20, 30);
        Assert(unchanged.IsSuccess && core.ImageMoves.Count == 0, "unchanged position must not enter history");
        var moved = await facade.MoveImageAsync(session.SessionId, image, 0, -10);
        Assert(moved.IsSuccess && moved.Value!.CanUndo && core.RefreshPreviewCalls == 1, "moving must accept zero/negative coordinates, refresh and enter history");
        Assert(core.ImageMoves[0].Item == source && core.ImageMoves[0].Rect == new PdfCoreRect(0, -10, 100, 50), "moving must preserve size, identity and resource/inline source");
        await facade.UndoAsync(session.SessionId);
        await facade.RedoAsync(session.SessionId);
        Assert(core.RefreshPreviewCalls == 3, "undo and redo must rebuild the painted PDF");
    }
}

static async Task RefusesInvalidImageMovementAsync()
{
    var core = new FakeCore { PageImages = [new PdfCoreContentImage(8, 0, new PdfCoreRect(20, 30, 100, 50), "Im1")] };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
    var image = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
    foreach (var (x, y) in new[] { (double.NaN, 30d), (20d, double.PositiveInfinity), (double.NegativeInfinity, 30d) })
        Assert(!(await facade.MoveImageAsync(session.SessionId, image, x, y)).IsSuccess, "non-finite coordinates must be refused");
    core.LastDocument!.ContentEditingAllowed = false;
    Assert(!(await facade.MoveImageAsync(session.SessionId, image, 40, 60)).IsSuccess, "content permission must be checked at submission");
    core.LastDocument.ContentEditingAllowed = true;
    core.LastDocument.FullRewriteAllowed = false;
    Assert(!(await facade.MoveImageAsync(session.SessionId, image, 40, 60)).IsSuccess, "an unwritable content edit must be refused");
    core.LastDocument.FullRewriteAllowed = true;
    Assert(core.ImageMoves.Count == 0 && core.RefreshPreviewCalls == 0, "refusals must not record or refresh");
    await facade.ResizeImageAsync(session.SessionId, image, 200, 75);
    Assert(!(await facade.MoveImageAsync(session.SessionId, image, 40, 60)).IsSuccess, "a snapshot preceding a resize must be refused");
    await facade.UndoAsync(session.SessionId);
    Assert(!(await facade.MoveImageAsync(session.SessionId, image, 40, 60)).IsSuccess, "undo must not revive an old snapshot");
    var fresh = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
    Assert((await facade.MoveImageAsync(session.SessionId, fresh, 40, 60)).IsSuccess, "re-reading must allow movement after undo");
    Assert(!(await facade.ResizeImageAsync(session.SessionId, fresh, 200, 75)).IsSuccess, "movement must invalidate resize snapshots too");
    var other = (await facade.OpenAsync(new DocumentSource("other.pdf", [2]), discardPendingEdits: true)).Value!;
    Assert(!(await facade.MoveImageAsync(other.SessionId, fresh, 40, 60)).IsSuccess, "another session must not accept the same image id");
    Assert(!(await facade.MoveImageAsync(session.SessionId, fresh, 40, 60)).IsSuccess, "a retired session must be refused");
    Assert(core.ImageMoves.Count == 1, "stale snapshots must never record another move");
}

static async Task KeepsImageMoveAfterPreviewFailureAsync()
{
    var core = new FakeCore { PageImages = [new PdfCoreContentImage(8, 0, new PdfCoreRect(20, 30, 100, 50), null)], RefreshPreviewThrows = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
    var image = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
    var result = await facade.MoveImageAsync(session.SessionId, image, 40, 60);
    Assert(!result.IsSuccess && core.LastDocument!.CanUndo && core.ImageMoves.Count == 1, "a preview failure must retain the undoable move");
}

static async Task ResizesContentImagesAsync()
{
    foreach (var resource in new string?[] { "Im1", null })
    {
        var source = new PdfCoreContentImage(8, 0, new PdfCoreRect(20, 30, 100, 50), resource);
        var core = new FakeCore { PageImages = [source] };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
        var image = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
        var unchanged = await facade.ResizeImageAsync(session.SessionId, image, 100, 50);
        Assert(unchanged.IsSuccess && core.ImageEdits.Count == 0, "unchanged dimensions must not enter history");
        var resized = await facade.ResizeImageAsync(session.SessionId, image, 200, 75);
        Assert(resized.IsSuccess && resized.Value!.CanUndo && core.RefreshPreviewCalls == 1, "resizing must refresh the preview and enter history");
        Assert(core.ImageEdits[0].Item == source && core.ImageEdits[0].Rect == new PdfCoreRect(20, 30, 200, 75), "the original identity and resource/inline source must survive; the origin must stay fixed");
        await facade.UndoAsync(session.SessionId);
        await facade.RedoAsync(session.SessionId);
        Assert(core.RefreshPreviewCalls == 3, "undo and redo must rebuild the painted PDF");
    }
}

static async Task RefusesInvalidImageResizingAsync()
{
    var core = new FakeCore { PageImages = [new PdfCoreContentImage(8, 0, new PdfCoreRect(20, 30, 100, 50), "Im1")] };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
    var image = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
    foreach (var (width, height) in new[] { (0d, 50d), (-1d, 50d), (double.NaN, 50d), (100d, double.PositiveInfinity) })
        Assert(!(await facade.ResizeImageAsync(session.SessionId, image, width, height)).IsSuccess, "invalid dimensions must be refused");
    core.LastDocument!.ContentEditingAllowed = false;
    Assert(!(await facade.ResizeImageAsync(session.SessionId, image, 200, 75)).IsSuccess, "content permission must be checked at submission");
    Assert(!(await facade.PageImagesAsync(session.SessionId, 0)).IsSuccess, "forbidden content must not be offered");
    core.LastDocument.ContentEditingAllowed = true;
    core.LastDocument.FullRewriteAllowed = false;
    Assert(!(await facade.ResizeImageAsync(session.SessionId, image, 200, 75)).IsSuccess, "an unwritable content edit must be refused");
    core.LastDocument.FullRewriteAllowed = true;
    Assert(core.ImageEdits.Count == 0 && core.RefreshPreviewCalls == 0, "refusals must not record or refresh");
    await facade.ResizeImageAsync(session.SessionId, image, 200, 75);
    Assert(!(await facade.ResizeImageAsync(session.SessionId, image, 300, 80)).IsSuccess, "a snapshot preceding an edit must be refused");
    await facade.UndoAsync(session.SessionId);
    Assert(!(await facade.ResizeImageAsync(session.SessionId, image, 300, 80)).IsSuccess, "undo must not revive an old snapshot");
    var other = (await facade.OpenAsync(new DocumentSource("other.pdf", [2]), discardPendingEdits: true)).Value!;
    Assert(!(await facade.ResizeImageAsync(other.SessionId, image, 300, 80)).IsSuccess, "another session must not accept the same image id");
    Assert(!(await facade.ResizeImageAsync(session.SessionId, image, 300, 80)).IsSuccess, "a retired session must be refused");
    Assert(core.ImageEdits.Count == 1, "stale snapshots must never record another edit");
}

static async Task KeepsImageResizeAfterPreviewFailureAsync()
{
    var core = new FakeCore { PageImages = [new PdfCoreContentImage(8, 0, new PdfCoreRect(20, 30, 100, 50), null)], RefreshPreviewThrows = true };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
    var image = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
    var result = await facade.ResizeImageAsync(session.SessionId, image, 200, 75);
    Assert(!result.IsSuccess && core.LastDocument!.CanUndo && core.ImageEdits.Count == 1, "a preview failure must report failure while retaining the undoable edit");
}

static async Task RefusesAFillForAMissingFieldAsync()
{
    var core = new FakeCore { FormFields = SampleFormFields() };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;

    var result = await facade.SetFormFieldValueAsync(session.SessionId, 99, new FormFieldValue.Text("Ada"));

    Assert(!result.IsSuccess && result.Error!.Message == "The document changed. Please try again.", "a stale field id must read as a changed document");
    Assert(!core.LastDocument!.CanUndo, "and must not enter history");
}

static async Task RefreshesThePreviewOnHistoryAfterAFillAsync()
{
    var core = new FakeCore { FormFields = SampleFormFields() };
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("form.pdf", [1]))).Value!;
    await facade.SetFormFieldValueAsync(session.SessionId, 1, new FormFieldValue.Checked(true));

    await facade.UndoAsync(session.SessionId);

    Assert(core.RefreshPreviewCalls == 2, "undoing a fill must drop the painted value from the preview");
}

static Task MapsDropdownChoicesToIndices()
{
    IReadOnlyList<string> options = ["S", "M", "L"];

    Assert(FormFieldChoices.IndexFor(options, null) == 0, "no choice is the leading \"(none)\" entry");
    Assert(FormFieldChoices.IndexFor(options, "M") == 2, "options sit one past their own index");
    Assert(FormFieldChoices.IndexFor(options, "XL") == 0, "a value outside the list shows as no choice");
    Assert(FormFieldChoices.ChoiceFor(options, 0) is null, "picking \"(none)\" clears the choice");
    Assert(FormFieldChoices.ChoiceFor(options, 3) == "L", "picking an option chooses it");
    Assert(FormFieldChoices.ChoiceFor(options, -1) is null && FormFieldChoices.ChoiceFor(options, 9) is null, "an out-of-range index chooses nothing");
    return Task.CompletedTask;
}

static Task NormalizesMultilineFieldText()
{
    // WinUI's TextBox stores a typed break as "\r"; pdf-form wraps paragraphs
    // on "\n" and nothing else.
    Assert(FormFieldText.FromTextBox("one\rtwo") == "one\ntwo", "a TextBox break must reach the core as \\n");
    Assert(FormFieldText.FromTextBox("one\r\ntwo") == "one\ntwo", "a pasted CRLF must become one break, not two");
    Assert(FormFieldText.ToTextBox("one\ntwo") == "one\rtwo", "a stored break must show as a TextBox break");
    Assert(FormFieldText.FromTextBox(FormFieldText.ToTextBox("a\nb\n")) == "a\nb\n", "a value must survive the round trip");
    return Task.CompletedTask;
}

static async Task RefusesPageContentAfterSessionSwapAsync()
{
    var core = new FakeCore();
    using var facade = new PdfDocumentFacade(core, new RecordingLogger());
    var session = (await facade.OpenAsync(new DocumentSource("first.pdf", [1]))).Value!;
    await facade.OpenAsync(new DocumentSource("second.pdf", [2]));

    var result = await facade.PageContentAsync(session.SessionId, 0);

    Assert(!result.IsSuccess, "run ids belong to the bytes they were parsed from, not to whatever is open now");
    Assert(result.Error!.Message == "The document is no longer available.", "the failure should be user safe");
}

static Task PicksTheSmallestRunUnderTheClick()
{
    var word = new PdfCoreContentTextRun(1, 0, new PdfCoreRect(100, 700, 40, 12), "F1", PdfCoreFontKind.Standard14, "Hello");
    var line = new PdfCoreContentTextRun(2, 0, new PdfCoreRect(90, 695, 300, 24), "F1", PdfCoreFontKind.Standard14, "Hello world and then some");
    IReadOnlyList<ContentTextRun> runs = [ContentRun(line), ContentRun(word)];

    Assert(ContentHitTest.TextRunAt(runs, Parsed, 110, 705)!.Id == 1, "overlapping runs resolve to the smaller, more specific one");
    Assert(ContentHitTest.TextRunAt(runs, Parsed, 350, 705)!.Id == 2, "a point only the wide run covers resolves to it");
    Assert(ContentHitTest.TextRunAt(runs, Parsed, 50, 400) is null, "a point outside every run resolves to nothing");

    // A retyped run no longer occupies the box it was parsed with, and the
    // reader points at what the page shows now.
    var stretched = (ContentTextRun run) => run.Id == 1
        ? new AnnotationRect(100, 700, 200, 12)
        : run.Bounds;
    Assert(ContentHitTest.TextRunAt(runs, Parsed, 250, 705)!.Id == 2, "the parsed box stops where it was parsed");
    Assert(ContentHitTest.TextRunAt(runs, stretched, 250, 705)!.Id == 1, "a run that grew is picked up over its whole new width");
    return Task.CompletedTask;
}

static Task MatchesPdfFontsToLocalFaces()
{
    // The three Standard-14 families are PostScript names for faces Windows
    // ships under other names — the only ones worth translating.
    Assert(PdfFontMatch.ForBaseFont("Helvetica").Families.StartsWith("Arial", StringComparison.Ordinal), "Helvetica is Arial here");
    Assert(PdfFontMatch.ForBaseFont("Times-Roman").Families.StartsWith("Times New Roman", StringComparison.Ordinal), "Times is Times New Roman here");
    Assert(PdfFontMatch.ForBaseFont("Courier").Families.StartsWith("Courier New", StringComparison.Ordinal), "Courier is Courier New here");

    // A subset tag says what was embedded, not which face to ask for.
    Assert(PdfFontMatch.ForBaseFont("ABCDEF+Times-BoldItalic").Families.StartsWith("Times New Roman", StringComparison.Ordinal), "the subset prefix is dropped");
    var styled = PdfFontMatch.ForBaseFont("ABCDEF+Times-BoldItalic");
    Assert(styled.Bold && styled.Italic, "the style the name declares is carried across");

    // Anything else is asked for by its own name, with the neutral face
    // behind it, so an installed font is used and a missing one falls back.
    var embedded = PdfFontMatch.ForBaseFont("Georgia,Bold");
    Assert(embedded.Families == $"Georgia, {PdfFontMatch.Fallback}", "an unknown family keeps its name and gains a fallback");
    Assert(embedded.Bold && !embedded.Italic, "comma-spelled styles are read too");

    Assert(PdfFontMatch.ForBaseFont(null).Families == PdfFontMatch.Fallback, "a page that does not say gets the fallback");
    return Task.CompletedTask;
}

static ContentTextRun ContentRun(PdfCoreContentTextRun source) => new(source, null);

/// <summary>Runs sitting exactly where they were parsed — nothing retyped yet.</summary>
static AnnotationRect Parsed(ContentTextRun run) => run.Bounds;

// ---------------------------------------------------------------------
// Content-edit pump — see `Viewer/ContentEditPump.cs`.
//
// Every rule here is about the order two continuations run in on the
// dispatcher thread, so these drive `TestDispatcher` by hand rather than
// awaiting: an ambient thread pool would run the same continuations in
// whatever order it liked and prove nothing about the one order the shell
// actually has.
// ---------------------------------------------------------------------

static Task OpeningAnEditorClosesTheOneAlreadyOpen()
{
    var pump = new ContentEditPump<FakeBox>(new GatedWriter(), new FakePause());
    var first = new FakeBox(0, 1, "Hello", "Hello");
    var second = new FakeBox(0, 2, "World", "World");

    // Two of these overlap whenever the first parks — on a page's first
    // content load, or on a commit still draining — because the shell opens
    // them from a pointer handler that cannot await.
    pump.Open(first);
    pump.Open(second);

    Assert(first.Closed, "an editor that stops being current must come off the page, not linger as an orphan");
    Assert(first.Closes == 1, "and exactly once");
    Assert(!second.Closed, "the newest one stays open");
    Assert(ReferenceEquals(pump.Box, second), "and is the current one");
    return Task.CompletedTask;
}

static Task CommitDrainsAWriteStartedWhileItWasWaiting()
{
    TestDispatcher.Run(dispatcher =>
    {
        var writer = new GatedWriter();
        var pump = new ContentEditPump<FakeBox>(writer, new FakePause());
        var box = new FakeBox(0, 7, "Hello", "Hello") { Text = "Hello " };
        pump.Open(box);

        var pumping = pump.PumpAsync();
        Assert(writer.Sent.Count == 1, "the first pass sends what the box says");

        // The reader keeps typing while that write is in flight, then commits.
        box.Text = "Hello world";
        var commit = pump.CommitAsync();
        dispatcher.Drain();
        Assert(!commit.IsCompleted, "the commit waits on the write already in flight");

        writer.Answer(ContentWriteOutcome.Written);
        dispatcher.Drain();

        // The pump registered its continuation first, so it woke first, saw
        // the new keystrokes and started a second write — one the commit has
        // no handle on.
        Assert(writer.Sent.Count == 2, "the pump picks up what arrived during the write");
        Assert(!commit.IsCompleted, "and the commit keeps waiting: it never saw that second write start");
        Assert(!box.Closed, "closing here would strand a write nobody is watching");

        writer.Answer(ContentWriteOutcome.Written);
        dispatcher.Drain();

        Assert(commit.IsCompleted, "once the document has caught up, the commit is done");
        Assert(box.Closed, "and only then does the box come off the page");
        Assert(pumping.IsCompleted, "the pump loop ends with it");
    });
    return Task.CompletedTask;
}

static Task ARefusalOnAWriteTheCommitNeverSawKeepsTheBoxOpen()
{
    TestDispatcher.Run(dispatcher =>
    {
        var writer = new GatedWriter();
        var pump = new ContentEditPump<FakeBox>(writer, new FakePause());
        var box = new FakeBox(0, 7, "Hello", "Hello") { Text = "Hello " };
        pump.Open(box);

        var pumping = pump.PumpAsync();
        // U+4E16 is outside every simple-font encoding, so the core declines
        // it. The reader types it during the first write and presses Enter.
        box.Text = "Hello 世";
        var commit = pump.CommitAsync();
        dispatcher.Drain();

        writer.Answer(ContentWriteOutcome.Written);
        dispatcher.Drain();
        writer.Answer(ContentWriteOutcome.Refused);
        dispatcher.Drain();

        Assert(commit.IsCompleted, "the commit finishes");
        Assert(!box.Closed, "but the box stays: typing something else is the only way a reader clears a refusal");
        Assert(ReferenceEquals(pump.Box, box), "and it is still the current editor");
        Assert(pump.Refused, "the pump waits for different text rather than re-sending the rejection");
        Assert(pumping.IsCompleted, "the loop stops on the refusal");
    });
    return Task.CompletedTask;
}

static Task SettlingForHistoryLandsTheWritesThenClosesTheBox()
{
    TestDispatcher.Run(dispatcher =>
    {
        var writer = new GatedWriter();
        var pump = new ContentEditPump<FakeBox>(writer, new FakePause());
        var box = new FakeBox(0, 7, "Hello", "Hello") { Text = "Goodbye" };
        pump.Open(box);

        var pumping = pump.PumpAsync();
        var settling = pump.SettleForHistoryAsync();
        dispatcher.Drain();

        Assert(!settling.IsCompleted, "a history step waits for the write already on its way");
        Assert(!box.Closed, "closing first would let that write land after the step and undo it");

        writer.Answer(ContentWriteOutcome.Written);
        dispatcher.Drain();

        Assert(settling.IsCompleted, "then the step may run");
        Assert(box.Closed && pump.Box is null, "and the box is gone before the log moves");
        Assert(pumping.IsCompleted, "the pump loop is done");
    });
    return Task.CompletedTask;
}

static Task AHistoryStepLeavesNothingThatCouldWriteTheUndoneTextBack()
{
    TestDispatcher.Run(dispatcher =>
    {
        var writer = new GatedWriter();
        var pump = new ContentEditPump<FakeBox>(writer, new FakePause());
        var box = new FakeBox(0, 7, "Hello", "Hello") { Text = "Goodbye" };
        pump.Open(box);

        var pumping = pump.PumpAsync();
        writer.Answer(ContentWriteOutcome.Written);
        dispatcher.Drain();
        Assert(pump.WrittenFor(0, 7) == "Goodbye", "the typing reached the document");

        // The reader clicks Undo. The step settles the editor first, and the
        // shell then forgets what was written, because nothing here knows
        // which command the step moved.
        var settling = pump.SettleForHistoryAsync();
        dispatcher.Drain();
        pump.Forget();

        // Whatever is pressed next, there is no box left to compare against
        // the restored text — which is what used to write the undone edit
        // straight back, with the redo stack cleared behind it.
        var commit = pump.CommitAsync();
        dispatcher.Drain();

        Assert(writer.Sent.Count == 1, "no second write: the undone text has nowhere to come from");
        Assert(pump.WrittenFor(0, 7) is null, "and the run prefills from the parsed page again");
        Assert(settling.IsCompleted && commit.IsCompleted, "both settle");
        Assert(pumping.IsCompleted, "the pump loop is done");
    });
    return Task.CompletedTask;
}

static Task AbandoningAnEditThisSessionCreatedAsksForAnUndo()
{
    TestDispatcher.Run(dispatcher =>
    {
        var writer = new GatedWriter();
        var pump = new ContentEditPump<FakeBox>(writer, new FakePause());
        // Opened on the page's own text, so the queued command belongs to this
        // session and undo removes it.
        var box = new FakeBox(0, 7, "Hello", "Hello") { Text = "Goodbye" };
        pump.Open(box);

        var pumping = pump.PumpAsync();
        writer.Answer(ContentWriteOutcome.Written);
        dispatcher.Drain();

        var abandoning = pump.AbandonAsync();
        dispatcher.Drain();

        Assert(abandoning.IsCompleted && abandoning.Result == ContentEditAbandon.Undo, "the way back is an undo");
        Assert(writer.Sent.Count == 1, "nothing is written back");
        Assert(pump.WrittenFor(0, 7) is null, "and the run stops claiming an unsaved retype");
        Assert(box.Closed && pumping.IsCompleted, "the box goes with it");
    });
    return Task.CompletedTask;
}

static Task AbandoningAnEditOnTopOfAnEarlierOneWritesTheEarlierTextBack()
{
    TestDispatcher.Run(dispatcher =>
    {
        var writer = new GatedWriter();
        var pump = new ContentEditPump<FakeBox>(writer, new FakePause());
        // Opened on text an earlier unsaved edit put there: undo would throw
        // that edit away too, so the way back is to write it again.
        var box = new FakeBox(0, 7, "Hello", "Goodbye") { Text = "Goodbye moon" };
        pump.Open(box);

        var pumping = pump.PumpAsync();
        writer.Answer(ContentWriteOutcome.Written);
        dispatcher.Drain();

        var abandoning = pump.AbandonAsync();
        dispatcher.Drain();
        writer.Answer(ContentWriteOutcome.Written);
        dispatcher.Drain();

        Assert(abandoning.IsCompleted && abandoning.Result == ContentEditAbandon.Rewritten, "not an undo");
        Assert(writer.Sent[^1] == "Goodbye", "the earlier text is what went back");
        Assert(box.Text == "Goodbye", "and the box shows it");
        Assert(pump.WrittenFor(0, 7) == "Goodbye", "the document holds it too");
        Assert(box.Closed && pumping.IsCompleted, "then it closes");
    });
    return Task.CompletedTask;
}

static Task ADocumentGoingAwayStopsThePumpWithoutLatchingARefusal()
{
    TestDispatcher.Run(dispatcher =>
    {
        var writer = new GatedWriter();
        var pump = new ContentEditPump<FakeBox>(writer, new FakePause());
        var box = new FakeBox(0, 7, "Hello", "Hello") { Text = "Goodbye" };
        pump.Open(box);

        var pumping = pump.PumpAsync();
        writer.Answer(ContentWriteOutcome.Abandoned);
        dispatcher.Drain();

        Assert(pumping.IsCompleted, "the loop stops rather than re-sending into a document that is gone");
        Assert(writer.Sent.Count == 1, "exactly one attempt");
        Assert(!pump.Refused, "not a refusal: latching one would strand the next editor on a failure that was never the reader's");
        Assert(pump.Box is null, "and no editor is left current");
        Assert(!box.Closed, "the shell's own reset already took this box off a page that no longer exists");
    });
    return Task.CompletedTask;
}

static Task OnlyTheLatestTextIsSentWhenKeystrokesArriveDuringAWrite()
{
    TestDispatcher.Run(dispatcher =>
    {
        var writer = new GatedWriter();
        var pump = new ContentEditPump<FakeBox>(writer, new FakePause());
        var box = new FakeBox(0, 7, "Hello", "Hello") { Text = "H" };
        pump.Open(box);

        var pumping = pump.PumpAsync();
        // Three more keystrokes land while the first write is in flight. Each
        // write replaces the last rather than stacking on it, so only the last
        // of them means anything.
        box.Text = "He";
        box.Text = "Hel";
        box.Text = "Hell";
        writer.Answer(ContentWriteOutcome.Written);
        dispatcher.Drain();
        writer.Answer(ContentWriteOutcome.Written);
        dispatcher.Drain();

        Assert(writer.Sent.Count == 2, "two writes, not four");
        Assert(writer.Sent[0] == "H" && writer.Sent[1] == "Hell", "the first, then whatever the box said afterwards");
        Assert(pumping.IsCompleted, "the loop ends once the document has caught up");
    });
    return Task.CompletedTask;
}

static Task RetypingAfterARefusalLetsThePumpTryAgain()
{
    TestDispatcher.Run(dispatcher =>
    {
        var writer = new GatedWriter();
        var pump = new ContentEditPump<FakeBox>(writer, new FakePause());
        var box = new FakeBox(0, 7, "Hello", "Hello") { Text = "Hello 世" };
        pump.Open(box);

        var refused = pump.PumpAsync();
        writer.Answer(ContentWriteOutcome.Refused);
        dispatcher.Drain();
        Assert(pump.Refused, "the refusal latches");

        var ignored = pump.PumpAsync();
        dispatcher.Drain();
        Assert(writer.Sent.Count == 1, "so the same rejection is not re-sent on every pass");

        // A key goes down: whatever was refused is being retyped.
        pump.Retry();
        box.Text = "Hello world";
        var again = pump.PumpAsync();
        writer.Answer(ContentWriteOutcome.Written);
        dispatcher.Drain();

        Assert(writer.Sent.Count == 2 && writer.Sent[1] == "Hello world", "and the new text goes out");
        Assert(!pump.Refused, "the latch is clear");
        Assert(refused.IsCompleted && ignored.IsCompleted && again.IsCompleted, "every pass finished");
    });
    return Task.CompletedTask;
}

static Task CommittingStopsThePauseBeforeItWaits()
{
    var pause = new FakePause();
    var pump = new ContentEditPump<FakeBox>(new GatedWriter(), pause);

    pump.Schedule();
    Assert(pause.Running, "typing arms the pause");

    var commit = pump.CommitAsync();

    Assert(commit.IsCompleted, "with no editor open there is nothing to wait for");
    Assert(!pause.Running, "and the pause is stopped, or it would fire into an editor that has already closed");
    return Task.CompletedTask;
}

sealed class FakeCore : IPdfCore
{
    public uint PageCount { get; init; } = 1;
    public double PageWidthPt { get; init; } = 595;
    public double PageHeightPt { get; init; } = 842;
    public PageRotation PageRotation { get; init; } = PageRotation.None;
    public List<PagePlacement> PlacementRequests { get; } = [];
    public PdfCoreError? OpenError { get; init; }
    public string? RequiredPassword { get; init; }
    public PdfCoreError? RenderError { get; init; }
    public bool BlockFirstRender { get; init; }
    public bool BlockFirstSearch { get; init; }
    public bool BlockSave { get; init; }
    public bool SaveThrowsUnexpected { get; init; }
    public PdfCoreError? InsertStampError { get; init; }
    public PdfCoreError? StampPlacementError { get; set; }
    public System.Collections.Concurrent.ConcurrentQueue<(double X, double Y)> StampPlacementAnchors { get; } = new();
    public TaskCompletionSource FirstRenderStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
    public ManualResetEventSlim ReleaseFirstRender { get; } = new(false);
    public TaskCompletionSource FirstSearchStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
    public ManualResetEventSlim ReleaseFirstSearch { get; } = new(false);
    public TaskCompletionSource SaveStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
    public ManualResetEventSlim ReleaseSave { get; } = new(false);
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

        if (RequiredPassword is { } required && password != required)
        {
            throw new PdfCoreException(password is null ? PdfCoreError.PasswordRequired : PdfCoreError.WrongPassword, "sensitive diagnostic");
        }

        return LastDocument = new FakeDocument(PageCount, PageWidthPt, PageHeightPt, PageRotation) { ContentEditingAllowed = ContentEditingPermitted };
    }

    public (string Open, string Permissions)? LastOpenWithPasswords;

    public IPdfCoreDocument OpenWithPasswordsFromBytes(byte[] bytes, string openPassword, string permissionsPassword)
    {
        LastOpenWithPasswords = (openPassword, permissionsPassword);
        return LastDocument = new FakeDocument(PageCount, PageWidthPt, PageHeightPt, PageRotation) { ContentEditingAllowed = ContentEditingPermitted };
    }

    /// <summary>
    /// Deliberately ignores <see cref="PageCount"/>. That knob describes the
    /// document an <see cref="OpenFromBytes"/> returns; the core's create
    /// entrypoint always hands back exactly one page, and a fake that echoed
    /// the opened count would let a zero-page regression pass green — which is
    /// how T-063's "This document has no pages." shipped.
    /// </summary>
    public IPdfCoreDocument CreateBlank() => LastDocument = new FakeDocument(1, PageWidthPt, PageHeightPt, PageRotation);

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

    /// <summary>How many times <see cref="PageCharacters"/> was called, and with which page index.</summary>
    public System.Collections.Concurrent.ConcurrentQueue<uint> PageCharactersCalls { get; } = new();

    public IPdfCorePageCharacters PageCharacters(IPdfCoreDocument document, uint pageIndex)
    {
        PageCharactersCalls.Enqueue(pageIndex);
        return new FakePageCharacters();
    }

    public IReadOnlyList<PdfCoreAnnotation> Annotations(IPdfCoreDocument document) => ((FakeDocument)document).Annotations;

    /// <summary>The runs <see cref="ReadPageContent"/> reports, on every page.</summary>
    public IReadOnlyList<PdfCoreContentTextRun> PageTextRuns { get; init; } =
        [new PdfCoreContentTextRun(7, 0, new PdfCoreRect(100, 700, 120, 12), "F1", PdfCoreFontKind.Standard14, "Hello world")];

    /// <summary>
    /// Whether opened documents permit content changes. An <c>init</c>
    /// property rather than a flag flipped on the document afterwards,
    /// because the facade reads this permission once, when the session is
    /// created — exactly as the real core reports it, from the document's own
    /// <c>/P</c> bits.
    /// </summary>
    public bool ContentEditingPermitted { get; init; } = true;

    /// <summary>A character the fake's font cannot encode, mirroring the core's own refusal.</summary>
    public string? UnencodableCharacter { get; init; }

    public bool RefreshPreviewThrows { get; init; }

    public PdfCoreDocumentInfo DocumentInfo { get; set; } = new(null, null, null, null, null, null, null, null);

    public System.Collections.Concurrent.ConcurrentQueue<uint> PageContentReads { get; } = new();
    public int RefreshPreviewCalls;
    public IReadOnlyList<PdfCoreContentImage> PageImages { get; init; } = [];
    public List<PdfCoreEdit.ReplaceImageSource> ImageReplacements { get; } = [];
    public PdfCoreError? ImageSourceError { get; set; }
    public PdfCoreError? ImageReplacementError { get; set; }

    public byte[] ImageSourceBytes(IPdfCoreDocument document, PdfCoreContentImage image)
    {
        if (ImageSourceError is { } error) throw new PdfCoreException(error, "image readback refused");
        return [1, 2, 3];
    }
    public List<PdfCoreEdit.ResizeImage> ImageEdits { get; } = [];
    public List<PdfCoreEdit.MoveImage> ImageMoves { get; } = [];
    public List<PdfCoreEdit.RemoveImage> ImageRemovals { get; } = [];
    public List<PdfCoreEdit.RemoveTextRun> TextRemovals { get; } = [];
    public List<PdfCoreEdit.MoveTextRun> TextMoves { get; } = [];
    public List<PdfCoreEdit.InsertTextRun> TextInsertions { get; } = [];
    public List<PdfCoreEdit.InsertImage> ImageInsertions { get; } = [];

    public PdfCorePageContent ReadPageContent(IPdfCoreDocument document, uint pageIndex)
    {
        PageContentReads.Enqueue(pageIndex);
        return new PdfCorePageContent([.. PageTextRuns.Select(run => run with { PageIndex = pageIndex })], PageImages);
    }

    /// <summary>What <see cref="PageFontFamilies"/> reports, keyed by resource name.</summary>
    public IReadOnlyDictionary<string, string> FontFamilies { get; init; } =
        new Dictionary<string, string> { ["F1"] = "Helvetica" };

    public IReadOnlyDictionary<string, string> PageFontFamilies(IPdfCoreDocument document, uint pageIndex) => FontFamilies;

    public PdfCoreDocumentInfo ReadDocumentInfo(IPdfCoreDocument document) => DocumentInfo;

    public List<PdfCoreFormField> FormFields { get; init; } = [];

    public IReadOnlyList<PdfCoreFormField> ListFormFields(IPdfCoreDocument document) => [.. FormFields];

    public void RefreshPreview(IPdfCoreDocument document)
    {
        Interlocked.Increment(ref RefreshPreviewCalls);
        if (RefreshPreviewThrows) throw new PdfCoreException(PdfCoreError.RenderFailed, "preview refresh failed");
    }

    public bool AnnotationEditingAllowed(IPdfCoreDocument document) => ((FakeDocument)document).EditingAllowed;
    /// <summary>Like the core's: the <c>/P</c> bit and a rewrite that can keep the encryption.</summary>
    public bool ContentEditingAllowed(IPdfCoreDocument document) => ((FakeDocument)document) is var fake && fake.ContentEditingAllowed && fake.FullRewriteAllowed;
    /// <summary>Like the core's: both <c>/P</c> bits, and never the rewrite check.</summary>
    public bool FormFieldEditingAllowed(IPdfCoreDocument document) => ((FakeDocument)document) is var fake && fake.EditingAllowed && fake.ContentEditingAllowed;
    public bool CanUndo(IPdfCoreDocument document) => ((FakeDocument)document).CanUndo;
    public bool CanRedo(IPdfCoreDocument document) => ((FakeDocument)document).CanRedo;
    /// <summary>The content edits the facade handed the core, newest last.</summary>
    public List<PdfCoreEdit.ReplaceTextRun> ContentEdits { get; } = [];
    public List<PdfCoreEdit.ReplaceTextRunWithInsertedFont> SubstitutionEdits { get; } = [];
    /// <summary>The page edits the facade handed the core, newest last.</summary>
    public List<PdfCoreEdit> PageEdits { get; } = [];
    public List<PdfCoreEdit.AddTextField> TextFieldEdits { get; } = [];
    public List<PdfCoreEdit.AddCheckbox> CheckboxEdits { get; } = [];
    public List<PdfCoreEdit.AddRadioGroup> RadioGroupEdits { get; } = [];
    public List<PdfCoreEdit.AddDropdown> DropdownEdits { get; } = [];

    public void ApplyEdit(IPdfCoreDocument document, PdfCoreEdit edit)
    {
        var fake = (FakeDocument)document;
        if (edit is PdfCoreEdit.ReplaceImageSource replacementImage)
        {
            if (!ContentEditingAllowed(document)) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
            if (ImageReplacementError is { } error) throw new PdfCoreException(error, "replacement refused");
            ImageReplacements.Add(replacementImage);
            fake.Apply(edit);
            return;
        }
        if (edit is PdfCoreEdit.InsertImage insertImage)
        {
            if (!ContentEditingAllowed(document)) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
            ImageInsertions.Add(insertImage);
            fake.Apply(edit);
            return;
        }
        if (edit is PdfCoreEdit.InsertTextRun insertText)
        {
            if (!ContentEditingAllowed(document)) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
            TextInsertions.Add(insertText);
            fake.Apply(edit);
            return;
        }
        if (edit is PdfCoreEdit.MoveTextRun moveText)
        {
            if (!ContentEditingAllowed(document)) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
            TextMoves.Add(moveText);
            fake.Apply(edit);
            return;
        }
        if (edit is PdfCoreEdit.RemoveTextRun removeText)
        {
            if (!ContentEditingAllowed(document)) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
            TextRemovals.Add(removeText);
            fake.Apply(edit);
            return;
        }
        if (edit is PdfCoreEdit.RemoveImage removeImage)
        {
            if (!ContentEditingAllowed(document)) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
            ImageRemovals.Add(removeImage);
            fake.Apply(edit);
            return;
        }
        if (edit is PdfCoreEdit.MoveImage moveImage)
        {
            if (!ContentEditingAllowed(document)) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
            ImageMoves.Add(moveImage);
            fake.Apply(edit);
            return;
        }
        if (edit is PdfCoreEdit.ResizeImage resizeImage)
        {
            if (!ContentEditingAllowed(document)) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
            ImageEdits.Add(resizeImage);
            fake.Apply(edit);
            return;
        }
        if (edit is PdfCoreEdit.ReplaceTextRun replacement)
        {
            // The real core refuses the whole command before recording it, so
            // the fake does too — a facade that reported success here would be
            // green against a double that lies.
            if (!fake.ContentEditingAllowed) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
            if (UnencodableCharacter is { } character && replacement.After.Contains(character, StringComparison.Ordinal))
            {
                throw new PdfCoreException(PdfCoreError.EncodingGap, "EncodingGap", character);
            }

            ContentEdits.Add(replacement);
            fake.Apply(edit);
            return;
        }

        if (edit is PdfCoreEdit.ReplaceTextRunWithInsertedFont substitution)
        {
            if (!fake.ContentEditingAllowed) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
            if (UnencodableCharacter is { } character && substitution.After.Contains(character, StringComparison.Ordinal))
            {
                throw new PdfCoreException(PdfCoreError.EncodingGap, "EncodingGap", character);
            }

            SubstitutionEdits.Add(substitution);
            fake.Apply(edit);
            return;
        }

        if (edit is PdfCoreEdit.SetDocumentInfo metadata)
        {
            if (!fake.ContentEditingAllowed) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
            DocumentInfo = metadata.After;
            fake.Apply(edit);
            return;
        }

        if (edit is PdfCoreEdit.InsertBlankPage or PdfCoreEdit.RotatePage or PdfCoreEdit.RemovePage or PdfCoreEdit.MovePages)
        {
            // The assembly permission, as in the core's `apply_edit`.
            if (!fake.PageEditingAllowed) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "this document does not permit inserting, removing or rotating its pages");
            PageEdits.Add(edit);
            fake.Apply(edit);
            return;
        }

        if (edit is PdfCoreEdit.SetFieldValue fill)
        {
            // Bit 6 is the floor for filling, as in the core's `apply_edit`;
            // content editing is deliberately not consulted.
            if (!fake.EditingAllowed) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "form filling is not permitted");
            var index = FormFields.FindIndex(field => field.Id == fill.FieldId);
            if (index < 0) throw new PdfCoreException(PdfCoreError.FormFieldNotFound, "form field not found");
            FormFields[index] = FormFields[index] with { Value = fill.Value };
            fake.Apply(edit);
            return;
        }

        if (edit is PdfCoreEdit.MoveFormField moveField)
        {
            if (!fake.EditingAllowed || !fake.ContentEditingAllowed)
                throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "form moving is not permitted");
            var index = FormFields.FindIndex(field => field.Id == moveField.FieldId);
            if (index < 0) throw new PdfCoreException(PdfCoreError.FormFieldNotFound, "form field not found");
            FormFields[index] = FormFields[index] with { Rect = moveField.Rect };
            fake.Apply(edit);
            return;
        }

        if (edit is PdfCoreEdit.ResizeFormField resizeField)
        {
            if (!fake.EditingAllowed || !fake.ContentEditingAllowed)
                throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "form resizing is not permitted");
            var index = FormFields.FindIndex(field => field.Id == resizeField.FieldId);
            if (index < 0) throw new PdfCoreException(PdfCoreError.FormFieldNotFound, "form field not found");
            FormFields[index] = FormFields[index] with { Rect = resizeField.Rect };
            fake.Apply(edit);
            return;
        }

        if (edit is PdfCoreEdit.RenameFormField rename)
        {
            if (!fake.EditingAllowed || !fake.ContentEditingAllowed)
                throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "form renaming is not permitted");
            var index = FormFields.FindIndex(field => field.Id == rename.FieldId);
            if (index < 0) throw new PdfCoreException(PdfCoreError.FormFieldNotFound, "form field not found");
            FormFields[index] = FormFields[index] with { Name = rename.Name };
            fake.Apply(edit);
            return;
        }

        if (edit is PdfCoreEdit.RestyleFormField restyle)
        {
            if (!fake.EditingAllowed || !fake.ContentEditingAllowed)
                throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "form styling is not permitted");
            var index = FormFields.FindIndex(field => field.Id == restyle.FieldId);
            if (index < 0) throw new PdfCoreException(PdfCoreError.FormFieldNotFound, "form field not found");
            FormFields[index] = FormFields[index] with { Style = restyle.Style };
            fake.Apply(edit);
            return;
        }

        if (edit is PdfCoreEdit.AddTextField field)
        {
            if (!fake.EditingAllowed || !fake.ContentEditingAllowed)
                throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "form creation is not permitted");
            TextFieldEdits.Add(field);
            FormFields.Add(new PdfCoreFormField((ulong)FormFields.Count, field.PageIndex, "Text", new FormFieldKind.Text(false, null), new FormFieldValue.Text("")));
            fake.Apply(edit);
            return;
        }

        if (edit is PdfCoreEdit.AddCheckbox checkbox)
        {
            if (!fake.EditingAllowed || !fake.ContentEditingAllowed)
                throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "form creation is not permitted");
            CheckboxEdits.Add(checkbox);
            FormFields.Add(new PdfCoreFormField((ulong)FormFields.Count, checkbox.PageIndex, "Checkbox", new FormFieldKind.Checkbox(), new FormFieldValue.Checked(false)));
            fake.Apply(edit);
            return;
        }

        if (edit is PdfCoreEdit.AddRadioGroup radio)
        {
            if (!fake.EditingAllowed || !fake.ContentEditingAllowed)
                throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "form creation is not permitted");
            RadioGroupEdits.Add(radio);
            FormFields.Add(new PdfCoreFormField((ulong)FormFields.Count, radio.PageIndex, "RadioGroup",
                new FormFieldKind.RadioGroup(["Option 1", "Option 2"]), new FormFieldValue.Choice(null)));
            fake.Apply(edit);
            return;
        }

        if (edit is PdfCoreEdit.AddDropdown dropdown)
        {
            if (!fake.EditingAllowed || !fake.ContentEditingAllowed)
                throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "form creation is not permitted");
            DropdownEdits.Add(dropdown);
            FormFields.Add(new PdfCoreFormField((ulong)FormFields.Count, dropdown.PageIndex, "Dropdown",
                new FormFieldKind.Dropdown(["Option 1", "Option 2"], false), new FormFieldValue.Choice(null)));
            fake.Apply(edit);
            return;
        }

        if (!fake.EditingAllowed) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "annotation editing is not permitted");
        fake.Apply(edit);
    }
    public void InsertImageStamp(IPdfCoreDocument document, uint pageIndex, byte[] imageBytes, PdfCoreRect rect)
    {
        if (InsertStampError is { } error) throw new PdfCoreException(error, "invalid image");
        var fake = (FakeDocument)document;
        if (!fake.EditingAllowed) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "annotation editing is not permitted");
        fake.InsertStamp(pageIndex, rect);
    }
    /// <summary>
    /// Stands in for the core's aspect-ratio sizing. The real arithmetic is
    /// tested in `pdf_annotate::placement` — what matters on this side is that
    /// the shell asks for a rect instead of inventing one, so the fake returns
    /// a shape no shell-side constant would ever produce and records what it
    /// was asked.
    /// </summary>
    public PlacedRect PlaceRect(AnnotationRect rect, PagePlacement page)
    {
        PlacementRequests.Add(page);
        return new PlacedRect(1, 2, 3, 4);
    }

    public PlacedPoint PlacePoint(AnnotationPoint point, PagePlacement page)
    {
        PlacementRequests.Add(page);
        return new PlacedPoint(point.X + 1, point.Y + 1);
    }

    public AnnotationPoint PointToPdf(PlacedPoint point, PagePlacement page)
    {
        PlacementRequests.Add(page);
        return new AnnotationPoint(point.Left - 1, point.Top - 1);
    }

    public PdfCoreRect StampPlacement(byte[] imageBytes, double anchorX, double anchorY)
    {
        if (StampPlacementError is { } error) throw new PdfCoreException(error, "invalid image");
        StampPlacementAnchors.Enqueue((anchorX, anchorY));
        return new PdfCoreRect(anchorX, anchorY - 77.0, 77.0, 77.0);
    }

    public bool Undo(IPdfCoreDocument document) => ((FakeDocument)document).Undo();
    public bool Redo(IPdfCoreDocument document) => ((FakeDocument)document).Redo();
    /// <summary>Set to make the fake behave like a signed document.</summary>
    public bool SignedDocument;

    /// <summary>The acknowledgement the facade passed on the last save.</summary>
    public bool? LastSaveAcknowledgedSignatures;

    public bool WillInvalidateSignatures(IPdfCoreDocument document) => SignedDocument;
    public bool ProtectionWillInvalidateSignatures(IPdfCoreDocument document) => SignedDocument;

    public (string Open, string Permissions)? LastProtectionPasswords;

    public byte[] SaveToBytes(IPdfCoreDocument document, bool signaturesAcknowledged)
    {
        LastSaveAcknowledgedSignatures = signaturesAcknowledged;
        if (BlockSave)
        {
            SaveStarted.SetResult();
            ReleaseSave.Wait(TimeSpan.FromSeconds(5));
        }
        if (SaveThrowsUnexpected) throw new InvalidOperationException("save failed");
        // Mirrors the core: an unacknowledged save of a signed document is
        // refused rather than silently producing a broken signature.
        if (SignedDocument && !signaturesAcknowledged)
        {
            throw new PdfCoreException(PdfCoreError.SignaturesWouldBeInvalidated, "signed document");
        }
        return [1];
    }

    public byte[] ProtectToBytes(IPdfCoreDocument document, string openPassword, string permissionsPassword, bool signaturesAcknowledged)
    {
        LastProtectionPasswords = (openPassword, permissionsPassword);
        if (SignedDocument && !signaturesAcknowledged)
        {
            throw new PdfCoreException(PdfCoreError.SignaturesWouldBeInvalidated, "signed document");
        }
        return [2];
    }

    /// <summary>What the fake's compression gate answers; <c>null</c> lets it run.</summary>
    public string? CompressionBlocker;
    public PdfCoreCompressedSave CompressionOutput = new([3], 10, 10, 0, false, []);
    public PdfCoreCompressPreset? LastCompressPreset;
    public bool? LastCompressAcknowledgedSignatures;

    public string? CompressionRefusal(IPdfCoreDocument document) => CompressionBlocker;
    public bool CompressedSaveWillInvalidateSignatures(IPdfCoreDocument document) => SignedDocument;

    public PdfCoreCompressedSave SaveCompressedToBytes(IPdfCoreDocument document, PdfCoreCompressPreset preset, bool signaturesAcknowledged)
    {
        LastCompressPreset = preset;
        LastCompressAcknowledgedSignatures = signaturesAcknowledged;
        if (SignedDocument && !signaturesAcknowledged)
        {
            throw new PdfCoreException(PdfCoreError.SignaturesWouldBeInvalidated, "signed document");
        }
        return CompressionOutput;
    }

    /// <summary>What <see cref="TextExtractionAllowed"/> answers — the document's <c>/P</c> bit 5.</summary>
    public bool ExtractionPermitted { get; init; } = true;
    /// <summary>The pages a custom range parses to; <c>null</c> reads every page.</summary>
    public uint[]? ParsedSelection { get; init; }
    /// <summary>When set, the core refuses any custom range with this sentence.</summary>
    public string? SelectionRefusal { get; init; }
    /// <summary>The page the core calls too large to raster; <c>null</c> when every page fits.</summary>
    public uint? OversizedPage { get; init; }
    public (string Input, uint TotalPages)? LastSelection;
    public (IReadOnlyList<uint> Pages, uint Dpi)? LastOversizeQuery;
    public System.Collections.Concurrent.ConcurrentQueue<(uint Page, uint Dpi, PdfCoreImageFormat Format)> ExportedPages { get; } = new();

    public bool TextExtractionAllowed(IPdfCoreDocument document) => ExtractionPermitted;

    public IReadOnlyList<uint> ParsePageSelection(string input, uint totalPages)
    {
        LastSelection = (input, totalPages);
        if (SelectionRefusal is { } sentence) throw new PdfCoreException(PdfCoreError.InvalidPageSelection, "InvalidPageSelection", sentence);
        return ParsedSelection ?? [.. Enumerable.Range(0, (int)totalPages).Select(page => (uint)page)];
    }

    public string PageImageFileName(string documentName, uint pageIndex, uint totalPages, PdfCoreImageFormat format) =>
        $"{documentName}|{pageIndex}|{totalPages}|{format}";

    public uint? FirstPageTooLargeToExport(IPdfCoreDocument document, IReadOnlyList<uint> pages, uint dpi)
    {
        LastOversizeQuery = (pages, dpi);
        return OversizedPage is { } page && pages.Contains(page) ? page : null;
    }

    public byte[] ExportPageImage(IPdfCoreDocument document, uint pageIndex, uint dpi, PdfCoreImageFormat format)
    {
        // The real core refuses before rendering, so the fake does too.
        if (!ExtractionPermitted) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "extraction is not permitted");
        ExportedPages.Enqueue((pageIndex, dpi, format));
        return format == PdfCoreImageFormat.Jpeg ? [0xFF, 0xD8] : [0x89, 0x50];
    }

    /// <summary>What <see cref="FullRewriteAllowed"/> answers — the second extract gate.</summary>
    public bool RewriteAllowed { get; init; } = true;
    /// <summary>What <see cref="ExtractSourceIsSigned"/> answers.</summary>
    public bool ExtractedSourceSigned { get; init; }
    public IReadOnlyList<uint>? LastExtractedPages;

    public bool FullRewriteAllowed(IPdfCoreDocument document) => RewriteAllowed;

    public byte[] ExtractPagesToPdf(IPdfCoreDocument document, IReadOnlyList<uint> pages)
    {
        // The real core refuses before pruning, so the fake does too.
        if (!ExtractionPermitted) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "extraction is not permitted");
        if (!RewriteAllowed) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "document cannot be fully rewritten");
        if (pages.Count == 0) throw new PdfCoreException(PdfCoreError.InvalidPageSelection, "InvalidPageSelection", "no pages were selected to extract");
        LastExtractedPages = pages;
        return [.. Enumerable.Repeat((byte)0x25, pages.Count)];
    }

    public bool ExtractSourceIsSigned(IPdfCoreDocument document) => ExtractedSourceSigned;

    public (string Cuts, uint Total, string Name)? LastSplitRequest;

    public IReadOnlyList<SplitPart> PlanSplit(string cuts, uint totalPages, string documentName)
    {
        LastSplitRequest = (cuts, totalPages, documentName);
        return [new SplitPart(0, 2, "report-part1.pdf"), new SplitPart(3, totalPages - 1, "report-part2.pdf")];
    }
}

/// <summary>
/// Stands in for one flattened page's characters: a single "A" glyph at
/// (100, 700), 12pt square — enough for a test to exercise caret/text/rect
/// queries without a real content stream.
/// </summary>
sealed class FakePageCharacters : IPdfCorePageCharacters
{
    public bool Disposed { get; private set; }
    public uint? CaretAt(double xPt, double yPt) => xPt <= 100 ? 0u : 1u;
    public string TextIn(uint anchor, uint focus) => anchor == focus ? "" : "A";
    public IReadOnlyList<PdfCoreSearchRect> RectsIn(uint anchor, uint focus) =>
        anchor == focus ? [] : [new PdfCoreSearchRect(100, 700, 12, 12)];
    public void Dispose() => Disposed = true;
}

sealed class FakeDocument(uint pageCount, double widthPt = 595, double heightPt = 842, PageRotation rotation = PageRotation.None) : IPdfCoreDocument
{
    private readonly List<PdfCorePageDimensions> _pages =
        [.. Enumerable.Range(0, (int)pageCount).Select(_ => new PdfCorePageDimensions(widthPt, heightPt, rotation))];
    public uint PageCount => (uint)_pages.Count;
    /// <summary>Live, like the core's: a page edit changes what it reports.</summary>
    public IReadOnlyList<PdfCorePageDimensions> PageDimensions => [.. _pages];
    public bool PageEditingAllowed { get; set; } = true;

    /// <summary>Replaces the page model with one page per width, all 842pt tall.</summary>
    public void Widths(params double[] widths)
    {
        _pages.Clear();
        _pages.AddRange(widths.Select(width => new PdfCorePageDimensions(width, 842, PageRotation.None)));
    }
    public bool Disposed { get; private set; }
    public bool EditingAllowed { get; set; } = true;
    /// <summary>The <c>/P</c> modify-contents bit alone; see <see cref="FullRewriteAllowed"/>.</summary>
    public bool ContentEditingAllowed { get; set; } = true;
    /// <summary>
    /// False for an encrypted document a full rewrite could not re-encrypt
    /// (opened with one password of two, say): the core then refuses content
    /// editing, but not form-field editing.
    /// </summary>
    public bool FullRewriteAllowed { get; set; } = true;
    public List<PdfCoreAnnotation> Annotations { get; } = [];
    public bool CanUndo { get; private set; }
    public bool CanRedo { get; private set; }
    private ulong _nextAnnotationId;

    public void Apply(PdfCoreEdit edit)
    {
        switch (edit)
        {
            case PdfCoreEdit.Add add:
                Annotations.Add(new PdfCoreAnnotation(_nextAnnotationId++, add.PageIndex, add.Kind, add.Rect, add.Color, add.Points ?? []));
                break;
            case PdfCoreEdit.Remove remove:
                Annotations.RemoveAll(annotation => annotation.Id == remove.AnnotationId);
                break;
            case PdfCoreEdit.ReplaceTextRun:
            case PdfCoreEdit.ReplaceTextRunWithInsertedFont:
            case PdfCoreEdit.SetDocumentInfo:
            case PdfCoreEdit.SetFieldValue:
                // Page content is not mirrored on the model — the queued
                // command is the edit — so there is nothing to mutate here
                // beyond the history the facade reads back.
                break;
            case PdfCoreEdit.InsertBlankPage insert:
                _pages.Insert((int)insert.Index, insert.Orientation == PageOrientation.Landscape
                    ? new(842, 595, PageRotation.None) : new(595, 842, PageRotation.None));
                break;
            case PdfCoreEdit.RotatePage rotate:
                var turned = _pages[(int)rotate.PageIndex];
                var rotation = (PageRotation)((((int)turned.Rotation + rotate.DeltaDegrees / 90) % 4 + 4) % 4);
                _pages[(int)rotate.PageIndex] = Math.Abs(rotate.DeltaDegrees) % 180 == 90
                    ? new(turned.HeightPt, turned.WidthPt, rotation)
                    : turned with { Rotation = rotation };
                break;
            case PdfCoreEdit.RemovePage removePage:
                _pages.RemoveAt((int)removePage.PageIndex);
                break;
            case PdfCoreEdit.MovePages move:
                var block = _pages.GetRange((int)move.From, (int)move.Count);
                _pages.RemoveRange((int)move.From, (int)move.Count);
                _pages.InsertRange((int)move.To, block);
                break;
            case PdfCoreEdit.Restyle restyle:
                var index = Annotations.FindIndex(annotation => annotation.Id == restyle.AnnotationId);
                if (index < 0) throw new PdfCoreException(PdfCoreError.AnnotationNotFound, "annotation not found");
                var annotation = Annotations[index];
                Annotations[index] = annotation with { Color = restyle.Color };
                break;
        }
        CanUndo = true;
        CanRedo = false;
    }
    public void InsertStamp(uint pageIndex, PdfCoreRect rect)
    {
        Annotations.Add(new PdfCoreAnnotation(_nextAnnotationId++, pageIndex, PdfCoreAnnotationKind.Stamp, rect, null, []));
        CanUndo = true;
        CanRedo = false;
    }
    public bool Undo() { if (!CanUndo) return false; CanUndo = false; CanRedo = true; return true; }
    public bool Redo() { if (!CanRedo) return false; CanRedo = false; CanUndo = true; return true; }
    public void Dispose() => Disposed = true;
}

sealed class RecordingLogger : IDiagnosticLogger
{
    public void Failure(PdfCoreError category, string operation, string correlationId, string? sessionId, uint? pageIndex, string sanitizedDetail) { }
}

// ---------------------------------------------------------------------
// Content-edit pump
//
// Every rule here is about the order two continuations run in on the
// dispatcher thread, so every test drives `TestDispatcher` by hand instead
// of awaiting: an ambient thread pool would run the same continuations in
// whatever order it liked and prove nothing about the one order the shell
// actually has.
// ---------------------------------------------------------------------

/// <summary>
/// A stand-in for the UI dispatcher: one queue, continuations run in the
/// order they were registered, and only when the test says so.
/// </summary>
sealed class TestDispatcher : SynchronizationContext
{
    private readonly Queue<(SendOrPostCallback Callback, object? State)> _queue = new();

    public override void Post(SendOrPostCallback d, object? state) => _queue.Enqueue((d, state));

    public override void Send(SendOrPostCallback d, object? state) => d(state);

    /// <summary>Runs queued continuations, including any they queue in turn.</summary>
    public void Drain()
    {
        while (_queue.Count > 0)
        {
            var (callback, state) = _queue.Dequeue();
            callback(state);
        }
    }

    /// <summary>Runs <paramref name="body"/> with this installed as the current context.</summary>
    public static void Run(Action<TestDispatcher> body)
    {
        var dispatcher = new TestDispatcher();
        var previous = Current;
        SetSynchronizationContext(dispatcher);
        try
        {
            body(dispatcher);
        }
        finally
        {
            SetSynchronizationContext(previous);
        }
    }
}

sealed class FakeBox(uint pageIndex, ulong runId, string runText, string openedWith) : IContentEditBox
{
    public uint PageIndex { get; } = pageIndex;
    public ulong RunId { get; } = runId;
    public string RunText { get; } = runText;
    public string OpenedWith { get; } = openedWith;
    public string Text { get; set; } = openedWith;
    public bool Closed { get; private set; }
    public int Closes { get; private set; }

    public void Close()
    {
        Closed = true;
        Closes++;
    }
}

/// <summary>
/// A writer whose every call parks until the test releases it, so a test can
/// stand exactly in the gap the pump's ordering rules are about.
/// </summary>
sealed class GatedWriter : IContentEditWriter<FakeBox>
{
    private readonly Queue<TaskCompletionSource<ContentWriteOutcome>> _gates = new();

    /// <summary>Every text handed to the writer, oldest first.</summary>
    public List<string> Sent { get; } = [];

    public int Waiting => _gates.Count;

    public Task<ContentWriteOutcome> WriteAsync(FakeBox box, string text)
    {
        Sent.Add(text);
        var gate = new TaskCompletionSource<ContentWriteOutcome>();
        _gates.Enqueue(gate);
        return gate.Task;
    }

    /// <summary>Answers the oldest write still waiting.</summary>
    public void Answer(ContentWriteOutcome outcome) => _gates.Dequeue().SetResult(outcome);
}

sealed class FakePause : IEditPause
{
    public int Stops { get; private set; }
    public int Restarts { get; private set; }
    public bool Running { get; private set; }

    public void Stop()
    {
        Stops++;
        Running = false;
    }

    public void Restart()
    {
        Restarts++;
        Running = true;
    }
}
