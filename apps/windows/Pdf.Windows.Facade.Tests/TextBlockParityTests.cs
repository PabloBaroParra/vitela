using Pdf.Windows.Facade;

/// <summary>
/// Text blocks through the facade (B25 T-207): the core's grouping arrives
/// with the page content holding the very run objects the page hands out,
/// and each block edit reaches the core as one command carrying the runs'
/// own snapshots.
/// </summary>
internal static class TextBlockParityTests
{
    private static readonly PdfCoreContentTextRun Presupues = new(0, 0, new PdfCoreRect(100, 697, 74, 12), "F1", PdfCoreFontKind.Standard14, "PRESUPUES");
    private static readonly PdfCoreContentTextRun To = new(1, 0, new PdfCoreRect(174, 697, 39, 12), "F1", PdfCoreFontKind.Standard14, "TO N 3");
    private static readonly PdfCoreContentTextRun Total = new(2, 0, new PdfCoreRect(300, 697, 27, 12), "F1", PdfCoreFontKind.Standard14, "Total");

    public static async Task RunAsync()
    {
        await BlocksArriveWithThePageContentAsync();
        await AMoveCarriesEveryRunAndTheOffsetAsync();
        await ARemovalAndARetypeEachReachTheCoreOnceAsync();
        await ACompositeFirstRunRetypesInTheInsertedFontAsync();
        await ARefusalIsReportedAndRefreshesNothingAsync();
        await NonFiniteOffsetsNeverReachTheCoreAsync();
    }

    private static async Task<(FakeCore Core, PdfDocumentFacade Facade, string Session)> OpenAsync(
        PdfCoreContentTextRun? first = null)
    {
        var heading = first ?? Presupues;
        var core = new FakeCore
        {
            PageTextRuns = [heading, To, Total],
            PageTextBlocks =
            [
                new([heading.Id, To.Id], new PdfCoreRect(100, 697, 113, 12), "PRESUPUESTO N 3"),
                new([Total.Id], Total.Bbox, "Total"),
            ],
        };
        var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("quote.pdf", [1]))).Value!;
        return (core, facade, session.SessionId);
    }

    private static async Task<ContentTextBlock> HeadingAsync(PdfDocumentFacade facade, string session) =>
        (await facade.PageContentAsync(session, 0)).Value!.TextBlocks[0];

    private static async Task BlocksArriveWithThePageContentAsync()
    {
        var (_, facade, session) = await OpenAsync();
        var content = (await facade.PageContentAsync(session, 0)).Value!;
        Assert(content.TextBlocks.Count == 2, "every core block arrives");
        var heading = content.TextBlocks[0];
        Assert(heading.IsMultiRun && heading.Text == "PRESUPUESTO N 3", "the heading is one block of two runs");
        Assert(ReferenceEquals(heading.Runs[0], content.TextRuns[0]) && ReferenceEquals(heading.Runs[1], content.TextRuns[1]),
            "a block holds the same run objects the page hands out");
        Assert(heading.Bounds == new AnnotationRect(100, 697, 113, 12), "a block keeps the box the core joined");
        Assert(ReferenceEquals(content.BlockOf(content.TextRuns[2]), content.TextBlocks[1]), "a run finds its own block");
        Assert(!content.TextBlocks[1].IsMultiRun, "a lone run is a block of one");
    }

    private static async Task AMoveCarriesEveryRunAndTheOffsetAsync()
    {
        var (core, facade, session) = await OpenAsync();
        var moved = await facade.MoveTextBlockAsync(session, await HeadingAsync(facade, session), 10, -20);
        Assert(moved.IsSuccess && moved.Value!.CanUndo && core.RefreshPreviewCalls == 1, "a block move refreshes and enters history");
        Assert(core.TextBlockEdits.Single() is PdfCoreTextBlockEdit.Move { Dx: 10, Dy: -20 } move
            && move.Runs.SequenceEqual([Presupues, To]), "a move carries every run's own snapshot and the offset");
    }

    private static async Task ARemovalAndARetypeEachReachTheCoreOnceAsync()
    {
        var (core, facade, session) = await OpenAsync();
        var heading = await HeadingAsync(facade, session);
        Assert((await facade.RemoveTextBlockAsync(session, heading)).IsSuccess, "a block removal is accepted");
        Assert((await facade.RetypeTextBlockAsync(session, heading, "FACTURA")).IsSuccess, "a block retype is accepted");
        Assert(core.TextBlockEdits.Count == 2
            && core.TextBlockEdits[0] is PdfCoreTextBlockEdit.Remove
            && core.TextBlockEdits[1] is PdfCoreTextBlockEdit.Retype { Text: "FACTURA", WithInsertedFont: false },
            "each block edit is one core command");
    }

    private static async Task ACompositeFirstRunRetypesInTheInsertedFontAsync()
    {
        var composite = Presupues with { FontKind = PdfCoreFontKind.EmbeddedComposite };
        var (core, facade, session) = await OpenAsync(composite);
        await facade.RetypeTextBlockAsync(session, await HeadingAsync(facade, session), "FACTURA");
        Assert(core.TextBlockEdits.Single() is PdfCoreTextBlockEdit.Retype { WithInsertedFont: true },
            "a composite first run is retyped in the inserted font");
    }

    private static async Task ARefusalIsReportedAndRefreshesNothingAsync()
    {
        var (core, facade, session) = await OpenAsync();
        core.TextBlockRefusal = "This text has an unsaved edit — save the document before editing it again.";
        var refused = await facade.MoveTextBlockAsync(session, await HeadingAsync(facade, session), 5, 5);
        Assert(!refused.IsSuccess && refused.Error!.Message.Contains("save the document"), "the core's reason reaches the shell");
        Assert(core.RefreshPreviewCalls == 0, "a refused edit rebuilds no preview");
    }

    private static async Task NonFiniteOffsetsNeverReachTheCoreAsync()
    {
        var (core, facade, session) = await OpenAsync();
        var refused = await facade.MoveTextBlockAsync(session, await HeadingAsync(facade, session), double.NaN, 0);
        Assert(!refused.IsSuccess && core.TextBlockEdits.Count == 0, "a NaN offset is refused before the core");
    }

    private static void Assert(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}

// Blocks come from PageTextBlocks, or one per run when a test sets none;
// edits are recorded in order and the model is not touched.
sealed partial class FakeCore
{
    public IReadOnlyList<PdfCoreTextBlock>? PageTextBlocks { get; init; }
    public List<PdfCoreTextBlockEdit> TextBlockEdits { get; } = [];
    public string? TextBlockRefusal { get; set; }

    public IReadOnlyList<PdfCoreTextBlock> ReadPageTextBlocks(IPdfCoreDocument document, uint pageIndex) =>
        PageTextBlocks ?? [.. PageTextRuns.Select(run => new PdfCoreTextBlock([run.Id], run.Bbox, run.Text))];

    public void ApplyTextBlockEdit(IPdfCoreDocument document, PdfCoreTextBlockEdit edit)
    {
        if (TextBlockRefusal is { } refusal) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "UnsupportedOperation", refusal);
        if (!((FakeDocument)document).ContentEditingAllowed)
            throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
        TextBlockEdits.Add(edit);
        // Page content is not mirrored on the model; recording a content edit
        // is what gives the facade the history a real block edit leaves.
        ((FakeDocument)document).Apply(new PdfCoreEdit.ReplaceTextRun(edit.Runs[0], edit.Runs[0].Text));
    }
}
