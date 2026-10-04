namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    public Task<OperationResult<DocumentBlocksSnapshot>> DocumentBlocksAsync(string sessionId)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
                return Task.FromResult(OperationResult<DocumentBlocksSnapshot>.Failure(CreateError(
                    "The document is no longer available.", PdfCoreError.DocumentNotFound, "document_blocks", sessionId, null)));
            try
            {
                return Task.FromResult(OperationResult<DocumentBlocksSnapshot>.Success(
                    new(session.Id, session.EditRevision, _core.DocumentBlocks(session.Document).ToArray())));
            }
            catch (PdfCoreException error) { return Task.FromResult(OperationResult<DocumentBlocksSnapshot>.Failure(MapError(error, "document_blocks", sessionId, null))); }
            catch (Exception error) { return Task.FromResult(OperationResult<DocumentBlocksSnapshot>.Failure(MapUnexpected(error, "document_blocks", sessionId, null))); }
        }
    }

    // Generated DocumentBlocks exposes positions, not stable PageIds. Validate the
    // entire revision-bound snapshot inside the mutation gate before using one.
    private PdfCoreEdit ResolveBlockEdit(SessionEntry session, PageEdit.Block edit)
    {
        var rows = _core.DocumentBlocks(session.Document);
        if (session.Id != edit.Snapshot.SessionId || session.EditRevision != edit.Snapshot.Revision || !rows.SequenceEqual(edit.Snapshot.Blocks)
            || edit.Position < 0 || edit.Position >= rows.Count)
            throw new PdfCoreException(PdfCoreError.PageIndexOutOfBounds, "The document changed. Please try again.");
        var block = rows[edit.Position];
        return edit.Action switch
        {
            DocumentBlockAction.RotateLeft => new PdfCoreEdit.RotatePages(block.Start, block.Count, -90),
            DocumentBlockAction.RotateRight => new PdfCoreEdit.RotatePages(block.Start, block.Count, 90),
            DocumentBlockAction.Delete => new PdfCoreEdit.RemovePages(block.Start, block.Count),
            DocumentBlockAction.Move => ResolveBlockMove(rows, edit.Position, edit.Slot),
            _ => throw new ArgumentOutOfRangeException(nameof(edit)),
        };
    }

    private PdfCoreEdit ResolveOrganizePageEdit(SessionEntry session, PageEdit.OrganizePage edit)
    {
        var rows = _core.DocumentBlocks(session.Document);
        if (session.Id != edit.Snapshot.SessionId || session.EditRevision != edit.Snapshot.Revision
            || !rows.SequenceEqual(edit.Snapshot.Blocks) || edit.Index >= session.Document.PageCount)
            throw new PdfCoreException(PdfCoreError.PageIndexOutOfBounds, "The document changed. Please try again.");
        return edit.Action switch
        {
            DocumentBlockAction.RotateLeft => new PdfCoreEdit.RotatePages(edit.Index, 1, -90),
            DocumentBlockAction.RotateRight => new PdfCoreEdit.RotatePages(edit.Index, 1, 90),
            DocumentBlockAction.Delete => new PdfCoreEdit.RemovePages(edit.Index, 1),
            DocumentBlockAction.Move when edit.Target < session.Document.PageCount && edit.Target != edit.Index
                => new PdfCoreEdit.MovePages(edit.Index, 1, edit.Target),
            _ => throw new PdfCoreException(PdfCoreError.PageIndexOutOfBounds, "Invalid page position."),
        };
    }

    private static PdfCoreEdit ResolveBlockMove(IReadOnlyList<DocumentBlock> rows, int position, int slot)
    {
        if (slot < 0 || slot > rows.Count || slot == position || slot == position + 1)
            throw new PdfCoreException(PdfCoreError.PageIndexOutOfBounds, "Invalid document position.");
        var block = rows[position];
        var before = rows.Take(slot).Aggregate(0u, (sum, row) => checked(sum + row.Count));
        return new PdfCoreEdit.MovePages(block.Start, block.Count, slot > position ? before - block.Count : before);
    }
}
