using Pdf.Windows.Facade;

/// <summary>
/// Multi-item selections through the facade (B25 T-208): a marquee gathers
/// every item whose box touches it, SHIFT+click toggles members, and a
/// selection edit reaches the core as one command carrying every member's
/// own snapshot.
/// </summary>
internal static class ContentSelectionParityTests
{
    private static readonly PdfCoreContentTextRun Cliente = new(0, 0, new PdfCoreRect(310, 650, 40, 12), "F1", PdfCoreFontKind.Standard14, "Cliente");
    private static readonly PdfCoreContentTextRun Domingo = new(1, 0, new PdfCoreRect(310, 630, 48, 12), "F1", PdfCoreFontKind.Standard14, "Domingo");
    private static readonly PdfCoreContentTextRun Footer = new(2, 0, new PdfCoreRect(300, 40, 30, 10), "F1", PdfCoreFontKind.Standard14, "Pag 1");
    private static readonly PdfCoreContentImage Logo = new(5, 0, new PdfCoreRect(100, 620, 60, 40), "Im1");
    private static readonly PdfCoreContentPath Frame = new(3, 0, new PdfCoreRect(300, 600, 200, 80), PdfCorePathPaint.Stroke);
    private static readonly PdfCoreContentPath Rule = new(4, 0, new PdfCoreRect(50, 500, 500, 0), PdfCorePathPaint.Stroke);

    public static async Task RunAsync()
    {
        await AMarqueeGathersWhatItTouchesAsync();
        await AMarqueeCatchesAHairlineRuleAsync();
        await ShiftClickTogglesMembersAsync();
        await AMoveCarriesEverySnapshotAndTheOffsetAsync();
        await ARemovalIsOneCoreCommandAsync();
        await ARefusalIsReportedAndRefreshesNothingAsync();
        await NonFiniteOffsetsNeverReachTheCoreAsync();
    }

    private static async Task<(FakeCore Core, PdfDocumentFacade Facade, string Session, PageContent Content)> OpenAsync()
    {
        var core = new FakeCore
        {
            PageTextRuns = [Cliente, Domingo, Footer],
            PageImages = [Logo],
            PagePaths = [Frame, Rule],
        };
        var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("quote.pdf", [1]))).Value!;
        var content = (await facade.PageContentAsync(session.SessionId, 0)).Value!;
        return (core, facade, session.SessionId, content);
    }

    private static AnnotationRect BlockBounds(ContentTextBlock block) => block.Bounds;

    private static async Task AMarqueeGathersWhatItTouchesAsync()
    {
        var (_, _, _, content) = await OpenAsync();
        // Starts inside the frame and ends past the logo: it only grazes the
        // frame's left edge region and the logo, never the footer.
        var picked = ContentSelection.Gather(content, new AnnotationRect(150, 610, 170, 50), BlockBounds);
        Assert(picked.Blocks.Select(block => block.Text).SequenceEqual(["Cliente", "Domingo"]), "both text blocks it touches");
        Assert(picked.Images.Single().Id == Logo.Id, "the logo it overlaps");
        Assert(picked.Paths.Single().Id == Frame.Id, "the frame it touches, not the rule below");
        Assert(picked.Bounds(BlockBounds) == new AnnotationRect(100, 600, 400, 80), "the selection's box is the union");
    }

    private static async Task AMarqueeCatchesAHairlineRuleAsync()
    {
        var (_, _, _, content) = await OpenAsync();
        var picked = ContentSelection.Gather(content, new AnnotationRect(200, 490, 20, 20), BlockBounds);
        Assert(picked.Count == 1 && picked.Paths.Single().Id == Rule.Id, "a zero-height rule is caught when crossed");
        Assert(ContentSelection.Gather(content, new AnnotationRect(0, 0, 5, 5), BlockBounds).IsEmpty, "an empty corner gathers nothing");
    }

    private static async Task ShiftClickTogglesMembersAsync()
    {
        var (_, _, _, content) = await OpenAsync();
        var selection = ContentSelection.Empty(0).Toggle(content.TextBlocks[0]).Toggle(content.Images[0]).Toggle(content.Paths[1]);
        Assert(selection.Count == 3, "three toggles in, three members");
        var fewer = selection.Toggle(content.Images[0]);
        Assert(fewer.Count == 2 && !fewer.Contains(content.Images[0]) && selection.Contains(content.Images[0]),
            "toggling a member out returns a new selection and leaves the old one alone");
    }

    private static async Task AMoveCarriesEverySnapshotAndTheOffsetAsync()
    {
        var (core, facade, session, content) = await OpenAsync();
        var selection = ContentSelection.Gather(content, new AnnotationRect(150, 610, 170, 50), BlockBounds);
        var moved = await facade.MoveContentSelectionAsync(session, selection, 12, -40);
        Assert(moved.IsSuccess && moved.Value!.CanUndo && core.RefreshPreviewCalls == 1, "a selection move refreshes and enters history");
        Assert(core.SelectionEdits.Single() is PdfCoreContentSelectionEdit.Move { Dx: 12, Dy: -40 } move
            && move.Runs.SequenceEqual([Cliente, Domingo]) && move.Images.SequenceEqual([Logo]) && move.Paths.SequenceEqual([Frame]),
            "the move carries every member's own snapshot and the offset");
    }

    private static async Task ARemovalIsOneCoreCommandAsync()
    {
        var (core, facade, session, content) = await OpenAsync();
        var everything = ContentSelection.Gather(content, new AnnotationRect(0, 0, 612, 792), BlockBounds);
        Assert((await facade.RemoveContentSelectionAsync(session, everything)).IsSuccess, "a selection removal is accepted");
        Assert(core.SelectionEdits.Single() is PdfCoreContentSelectionEdit.Remove { Runs.Count: 3, Images.Count: 1, Paths.Count: 2 },
            "every member goes in one command");
    }

    private static async Task ARefusalIsReportedAndRefreshesNothingAsync()
    {
        var (core, facade, session, content) = await OpenAsync();
        core.ContentSelectionRefusal = "A selection edits one page at a time.";
        var refused = await facade.MoveContentSelectionAsync(session, ContentSelection.Empty(0).Toggle(content.Images[0]), 1, 1);
        Assert(!refused.IsSuccess && refused.Error!.Message == "A selection edits one page at a time.", "the core's sentence reaches the shell");
        Assert(core.RefreshPreviewCalls == 0, "a refused edit rebuilds no preview");
    }

    private static async Task NonFiniteOffsetsNeverReachTheCoreAsync()
    {
        var (core, facade, session, content) = await OpenAsync();
        var refused = await facade.MoveContentSelectionAsync(session, ContentSelection.Empty(0).Toggle(content.Images[0]), 0, double.PositiveInfinity);
        Assert(!refused.IsSuccess && core.SelectionEdits.Count == 0, "an infinite offset is refused before the core");
    }

    private static void Assert(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}

// Selection edits are recorded in order; the page content is not mirrored.
sealed partial class FakeCore
{
    public List<PdfCoreContentSelectionEdit> SelectionEdits { get; } = [];
    public string? ContentSelectionRefusal { get; set; }

    public void ApplyContentSelectionEdit(IPdfCoreDocument document, PdfCoreContentSelectionEdit edit)
    {
        if (ContentSelectionRefusal is { } refusal) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "UnsupportedOperation", refusal);
        if (!((FakeDocument)document).ContentEditingAllowed)
            throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
        SelectionEdits.Add(edit);
        // Recording a content edit is what gives the facade the history a
        // real selection edit leaves.
        ((FakeDocument)document).Apply(new PdfCoreEdit.ReplaceTextRun(edit.Runs.Count > 0 ? edit.Runs[0] : PageTextRuns[0], ""));
    }
}
