using Pdf.Windows.Facade;

/// <summary>
/// The content clipboard through the facade (B25 T-209): a copy changes
/// nothing, a cut is one undo step that hands back its clip, and a paste
/// reaches the core with the clip and the cursor's corner.
/// </summary>
internal static class ContentClipboardParityTests
{
    private static readonly PdfCoreContentTextRun Cliente = new(0, 0, new PdfCoreRect(310, 650, 40, 12), "F1", PdfCoreFontKind.Standard14, "Cliente");
    private static readonly PdfCoreContentImage Logo = new(5, 0, new PdfCoreRect(100, 620, 60, 40), "Im1");
    private static readonly PdfCoreContentPath Frame = new(3, 0, new PdfCoreRect(300, 600, 200, 80), PdfCorePathPaint.Stroke);

    public static async Task RunAsync()
    {
        await ACopyChangesNothingAsync();
        await ACutIsOneUndoStepWithItsClipAsync();
        await APasteCarriesTheClipAndTheCornerAsync();
        await ARefusedCopyKeepsTheCoreSentenceAsync();
        await NonFiniteCornersNeverReachTheCoreAsync();
    }

    private static async Task<(FakeCore Core, PdfDocumentFacade Facade, string Session, ContentSelection Selection)> OpenAsync()
    {
        var core = new FakeCore
        {
            PageTextRuns = [Cliente],
            PageImages = [Logo],
            PagePaths = [Frame],
        };
        var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("quote.pdf", [1]))).Value!;
        var content = (await facade.PageContentAsync(session.SessionId, 0)).Value!;
        var selection = ContentSelection.Gather(content, new AnnotationRect(0, 0, 612, 792), block => block.Bounds);
        return (core, facade, session.SessionId, selection);
    }

    private static async Task ACopyChangesNothingAsync()
    {
        var (core, facade, session, selection) = await OpenAsync();
        var copied = await facade.CopyContentSelectionAsync(session, selection);
        Assert(copied.IsSuccess && copied.Value!.Bounds == new AnnotationRect(100, 600, 400, 80), "a copy hands back the clip's box");
        Assert(core.ClipboardCalls.Single() is ("copy", 1, 1, 1), "every member reaches the core once");
        Assert(core.RefreshPreviewCalls == 0, "a copy rebuilds no preview");
    }

    private static async Task ACutIsOneUndoStepWithItsClipAsync()
    {
        var (core, facade, session, selection) = await OpenAsync();
        var cut = await facade.CutContentSelectionAsync(session, selection);
        Assert(cut.IsSuccess && cut.Value!.State.CanUndo && core.RefreshPreviewCalls == 1, "a cut refreshes and enters history");
        Assert(core.ClipboardCalls.Single() is ("cut", 1, 1, 1), "the cut is one core call");
    }

    private static async Task APasteCarriesTheClipAndTheCornerAsync()
    {
        var (core, facade, session, selection) = await OpenAsync();
        var clip = (await facade.CopyContentSelectionAsync(session, selection)).Value!;
        var pasted = await facade.PasteContentAsync(session, 0, clip, 40, 500);
        Assert(pasted.IsSuccess && pasted.Value!.CanUndo && core.RefreshPreviewCalls == 1, "a paste refreshes and enters history");
        Assert(core.Pastes.Single() == (0u, clip.Source, 40.0, 500.0), "the paste carries the clip and the cursor's corner");
    }

    private static async Task ARefusedCopyKeepsTheCoreSentenceAsync()
    {
        var (core, facade, session, selection) = await OpenAsync();
        core.ClipboardRefusal = "Save the document before copying content you have edited.";
        var refused = await facade.CopyContentSelectionAsync(session, selection);
        Assert(!refused.IsSuccess && refused.Error!.Message == core.ClipboardRefusal, "the core's sentence reaches the shell");
    }

    private static async Task NonFiniteCornersNeverReachTheCoreAsync()
    {
        var (core, facade, session, selection) = await OpenAsync();
        var clip = (await facade.CopyContentSelectionAsync(session, selection)).Value!;
        var refused = await facade.PasteContentAsync(session, 0, clip, double.NaN, 10);
        Assert(!refused.IsSuccess && core.Pastes.Count == 0, "a NaN corner is refused before the core");
    }

    private static void Assert(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}

// Clipboard calls are recorded in order; the page content is not mirrored.
sealed partial class FakeCore
{
    public List<(string Kind, int Runs, int Images, int Paths)> ClipboardCalls { get; } = [];
    public List<(uint Page, PdfCoreContentClip Clip, double Left, double Top)> Pastes { get; } = [];
    public string? ClipboardRefusal { get; set; }

    public PdfCoreContentClip CopyContentSelection(IPdfCoreDocument document, PdfCoreContentSelectionEdit.Remove members, bool cut)
    {
        if (ClipboardRefusal is { } refusal) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "UnsupportedOperation", refusal);
        ClipboardCalls.Add((cut ? "cut" : "copy", members.Runs.Count, members.Images.Count, members.Paths.Count));
        if (cut) ((FakeDocument)document).Apply(new PdfCoreEdit.ReplaceTextRun(PageTextRuns[0], ""));
        var boxes = members.Runs.Select(run => run.Bbox).Concat(members.Images.Select(image => image.Bbox)).Concat(members.Paths.Select(path => path.Bbox)).ToArray();
        var left = boxes.Min(box => box.X);
        var bottom = boxes.Min(box => box.Y);
        return new PdfCoreContentClip([1, 2, 3], new PdfCoreRect(left, bottom,
            boxes.Max(box => box.X + box.Width) - left, boxes.Max(box => box.Y + box.Height) - bottom));
    }

    public void PasteContent(IPdfCoreDocument document, uint pageIndex, PdfCoreContentClip clip, double left, double top)
    {
        Pastes.Add((pageIndex, clip, left, top));
        ((FakeDocument)document).Apply(new PdfCoreEdit.ReplaceTextRun(PageTextRuns[0], ""));
    }
}
