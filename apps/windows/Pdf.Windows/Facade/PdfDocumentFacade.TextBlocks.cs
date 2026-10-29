namespace Pdf.Windows.Facade;

/// <summary>
/// Text blocks (B25 T-207): runs on one line that read as one piece of text,
/// moved, deleted and retyped as a unit. Each edit is one undo step in the
/// core, and the core folds a block edited again the same way — moved again,
/// or retyped again while the reader types — into the step already queued.
/// </summary>
/// <remarks>
/// Same posture as the single-run writes in <c>PdfDocumentFacade.cs</c>: the
/// runs travel back as the core's own snapshots, the edit is recorded, and
/// the preview is rebuilt before the result returns.
/// </remarks>
public sealed partial class PdfDocumentFacade
{
    /// <summary>Moves every run of <paramref name="block"/> by the same offset, in PDF points.</summary>
    public Task<OperationResult<AnnotationState>> MoveTextBlockAsync(string sessionId, ContentTextBlock block, double dx, double dy)
    {
        if (!double.IsFinite(dx) || !double.IsFinite(dy))
        {
            return Task.FromResult(OperationResult<AnnotationState>.Failure(CreateError("Text coordinates must be finite.", PdfCoreError.UnsupportedOperation, "move_text_block", sessionId, block.PageIndex)));
        }
        return WriteTextBlockAsync(sessionId, block, "move_text_block", new PdfCoreTextBlockEdit.Move(Sources(block), dx, dy));
    }

    /// <summary>Removes every run of <paramref name="block"/>.</summary>
    public Task<OperationResult<AnnotationState>> RemoveTextBlockAsync(string sessionId, ContentTextBlock block) =>
        WriteTextBlockAsync(sessionId, block, "remove_text_block", new PdfCoreTextBlockEdit.Remove(Sources(block)));

    /// <summary>
    /// Puts all of <paramref name="text"/> in the block's first run and removes
    /// the others. A composite-font first run is painted in the inserted
    /// standard font, which the caller confirms with the reader first.
    /// </summary>
    public Task<OperationResult<AnnotationState>> RetypeTextBlockAsync(string sessionId, ContentTextBlock block, string text) =>
        WriteTextBlockAsync(sessionId, block, "retype_text_block",
            new PdfCoreTextBlockEdit.Retype(Sources(block), text, block.First.RequiresFontSubstitution));

    private static PdfCoreContentTextRun[] Sources(ContentTextBlock block) => [.. block.Runs.Select(run => run.Source)];

    /// <summary>The core's blocks, holding the very run objects the page content hands out.</summary>
    private static ContentTextBlock[] TextBlocksOf(IReadOnlyList<PdfCoreTextBlock> blocks, IReadOnlyList<ContentTextRun> runs)
    {
        var byId = runs.ToDictionary(run => run.Id);
        return [.. blocks
            .Select(block => (block, members: block.RunIds.Select(id => byId.GetValueOrDefault(id)).OfType<ContentTextRun>().ToArray()))
            .Where(pair => pair.members.Length > 0)
            .Select(pair => new ContentTextBlock(pair.members,
                new AnnotationRect(pair.block.Bbox.X, pair.block.Bbox.Y, pair.block.Bbox.Width, pair.block.Bbox.Height),
                pair.block.Text))];
    }

    /// <summary>
    /// A refusal the core wrote as a full sentence (it ends in a period) is
    /// shown as is; a bare clause such as a permission name maps as usual.
    /// </summary>
    private UserSafeError MapTextBlockError(PdfCoreException error, string operation, string sessionId, uint pageIndex) =>
        error.Category == PdfCoreError.UnsupportedOperation && error.ReaderFacingDetail is { } reason && reason.EndsWith('.')
            ? CreateError(reason, error.Category, operation, sessionId, pageIndex)
            : MapError(error, operation, sessionId, pageIndex);

    private Task<OperationResult<AnnotationState>> WriteTextBlockAsync(
        string sessionId,
        ContentTextBlock block,
        string operation,
        PdfCoreTextBlockEdit edit) =>
        WriteContentBatchAsync(sessionId, block.PageIndex, operation, document => _core.ApplyTextBlockEdit(document, edit));

    /// <summary>
    /// Records one multi-item content edit — a text block's or a selection's —
    /// and rebuilds the preview. The core's own refusal sentence reaches the
    /// reader as is.
    /// </summary>
    private async Task<OperationResult<AnnotationState>> WriteContentBatchAsync(
        string sessionId,
        uint pageIndex,
        string operation,
        Action<IPdfCoreDocument> apply)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        SessionEntry session;
        try
        {
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                {
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, operation, sessionId, pageIndex));
                }

                try
                {
                    apply(session.Document);
                    session.EditRevision++;
                    session.HasRecordedPreviewEdit = true;
                }
                catch (PdfCoreException error)
                {
                    return OperationResult<AnnotationState>.Failure(MapTextBlockError(error, operation, sessionId, pageIndex));
                }
            }

            return await RefreshPreviewAsync(session, operation, pageIndex).ConfigureAwait(false);
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }
}
