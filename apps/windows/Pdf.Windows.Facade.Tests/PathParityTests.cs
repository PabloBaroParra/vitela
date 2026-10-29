using Pdf.Windows.Facade;

/// <summary>
/// Vector paths through the facade (B25 T-206): the frame of a table or a
/// rule is read with the page, moved, reshaped and deleted as one undoable
/// content edit each, and a stale snapshot never reaches the core.
/// </summary>
internal static class PathParityTests
{
    private static readonly PdfCoreContentPath Frame =
        new(3, 0, new PdfCoreRect(319, 135, 251, 125), PdfCorePathPaint.Stroke);

    public static async Task RunAsync()
    {
        await PathsArriveWithThePageContentAsync();
        await AMoveKeepsTheSizeAndEntersHistoryAsync();
        await ACornerDragIsOneResizeAsync();
        await ADeletionIsUndoableAsync();
        await AFreshReadAllowsASecondEditBeforeSavingAsync();
        await StaleAndForbiddenSnapshotsAreRefusedAsync();
        await BoundsThatMeanNothingAreRefusedBeforeTheCoreAsync();
    }

    private static async Task<(FakeCore Core, PdfDocumentFacade Facade, string Session)> OpenAsync()
    {
        var core = new FakeCore { PagePaths = [Frame] };
        var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("table.pdf", [1]))).Value!;
        return (core, facade, session.SessionId);
    }

    private static async Task<ContentPath> FirstPathAsync(PdfDocumentFacade facade, string session) =>
        (await facade.PagePathsAsync(session, 0)).Value![0];

    private static async Task PathsArriveWithThePageContentAsync()
    {
        var (_, facade, session) = await OpenAsync();
        var content = (await facade.PageContentAsync(session, 0)).Value!;
        Assert(content.Paths.Count == 1 && content.Paths[0].IsOutline, "the page content must carry its paths");
        Assert(content.Paths[0].Bounds == new AnnotationRect(319, 135, 251, 125), "a path keeps the box the core read");
    }

    private static async Task AMoveKeepsTheSizeAndEntersHistoryAsync()
    {
        var (core, facade, session) = await OpenAsync();
        var path = await FirstPathAsync(facade, session);
        var unchanged = await facade.MovePathAsync(session, path, 319, 135);
        Assert(unchanged.IsSuccess && core.PathEdits.Count == 0, "an unchanged position must not enter history");
        var moved = await facade.MovePathAsync(session, path, 20, 600);
        Assert(moved.IsSuccess && moved.Value!.CanUndo && core.RefreshPreviewCalls == 1, "a move must refresh and enter history");
        Assert(core.PathEdits.Single() is PdfCoreEdit.MovePath { Item: var item, To: var to }
            && item == Frame && to == new PdfCoreRect(20, 600, 251, 125), "a move carries the original snapshot and keeps the size");
    }

    private static async Task ACornerDragIsOneResizeAsync()
    {
        var (core, facade, session) = await OpenAsync();
        var path = await FirstPathAsync(facade, session);
        var resized = await facade.SetPathBoundsAsync(session, path, new AnnotationRect(300, 100, 280, 200));
        Assert(resized.IsSuccess, "a corner drag must be accepted");
        Assert(core.PathEdits.Single() is PdfCoreEdit.ResizePath { To: var to } && to == new PdfCoreRect(300, 100, 280, 200),
            "a corner drag is one resize, origin included");
    }

    private static async Task ADeletionIsUndoableAsync()
    {
        var (core, facade, session) = await OpenAsync();
        var path = await FirstPathAsync(facade, session);
        var deleted = await facade.RemovePathAsync(session, path);
        Assert(deleted.IsSuccess && core.PathEdits.Single() is PdfCoreEdit.RemovePath, "a deletion must be recorded");
        var undone = await facade.UndoAsync(session);
        Assert(undone.IsSuccess && undone.Value!.CanRedo, "a deletion must be undoable");
        Assert(!(await facade.RemovePathAsync(session, path)).IsSuccess, "undo must not revive the old snapshot");
    }

    private static async Task AFreshReadAllowsASecondEditBeforeSavingAsync()
    {
        var (core, facade, session) = await OpenAsync();
        await facade.MovePathAsync(session, await FirstPathAsync(facade, session), 20, 600);
        var again = await facade.MovePathAsync(session, await FirstPathAsync(facade, session), 40, 620);
        Assert(again.IsSuccess && core.PathEdits.Count == 2, "a re-read path can be moved again before saving");
    }

    private static async Task StaleAndForbiddenSnapshotsAreRefusedAsync()
    {
        var (core, facade, session) = await OpenAsync();
        var path = await FirstPathAsync(facade, session);
        core.LastDocument!.ContentEditingAllowed = false;
        Assert(!(await facade.MovePathAsync(session, path, 1, 1)).IsSuccess, "content permission must be checked at submission");
        core.LastDocument.ContentEditingAllowed = true;
        await facade.MovePathAsync(session, path, 20, 600);
        Assert(!(await facade.RemovePathAsync(session, path)).IsSuccess, "an intervening edit must invalidate the snapshot");
        var fresh = await FirstPathAsync(facade, session);
        var other = (await facade.OpenAsync(new DocumentSource("other.pdf", [2]), discardPendingEdits: true)).Value!;
        Assert(!(await facade.RemovePathAsync(other.SessionId, fresh)).IsSuccess, "another session must not accept the same path");
        Assert(core.PathEdits.Count == 1, "stale snapshots must never reach the core");
    }

    private static async Task BoundsThatMeanNothingAreRefusedBeforeTheCoreAsync()
    {
        var (core, facade, session) = await OpenAsync();
        var path = await FirstPathAsync(facade, session);
        Assert(!(await facade.MovePathAsync(session, path, double.NaN, 0)).IsSuccess, "a non-finite move must be refused");
        Assert(!(await facade.SetPathBoundsAsync(session, path, new AnnotationRect(0, 0, -5, 10))).IsSuccess, "a negative extent must be refused");
        Assert((await facade.SetPathBoundsAsync(session, path, new AnnotationRect(0, 0, 50, 0))).IsSuccess, "a flat box is a rule, and legal");
        Assert(core.PathEdits.Count == 1, "only the legal resize reaches the core");
    }

    private static void Assert(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}

// The core is faked at the boundary: paths come from PagePaths, edits are
// recorded in order, and the model is not touched — page content never is.
sealed partial class FakeCore
{
    public IReadOnlyList<PdfCoreContentPath> PagePaths { get; init; } = [];
    public List<PdfCoreEdit> PathEdits { get; } = [];

    public IReadOnlyList<PdfCoreContentPath> ReadPagePaths(IPdfCoreDocument document, uint pageIndex) =>
        [.. PagePaths.Select(path => path with { PageIndex = pageIndex })];

    private bool TryApplyPath(FakeDocument fake, PdfCoreEdit edit)
    {
        if (edit is not (PdfCoreEdit.MovePath or PdfCoreEdit.ResizePath or PdfCoreEdit.RemovePath)) return false;
        if (!fake.ContentEditingAllowed) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
        PathEdits.Add(edit);
        fake.Apply(edit);
        return true;
    }
}
