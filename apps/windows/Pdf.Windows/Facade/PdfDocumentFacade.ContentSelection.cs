namespace Pdf.Windows.Facade;

/// <summary>
/// Multi-item selections (B25 T-208): what a marquee or SHIFT+click gathers
/// on one page, moved or deleted as one undo step. The core folds a
/// selection dragged again into the step already queued.
/// </summary>
public sealed partial class PdfDocumentFacade
{
    /// <summary>Moves every member of <paramref name="selection"/> by the same offset, in PDF points.</summary>
    public Task<OperationResult<AnnotationState>> MoveContentSelectionAsync(string sessionId, ContentSelection selection,
        double dx, double dy)
    {
        if (!double.IsFinite(dx) || !double.IsFinite(dy))
        {
            return Task.FromResult(OperationResult<AnnotationState>.Failure(CreateError("Selection offsets must be finite.",
                PdfCoreError.UnsupportedOperation, "move_content_selection", sessionId, selection.PageIndex)));
        }
        var edit = new PdfCoreContentSelectionEdit.Move(selection.RunSources, selection.ImageSources, selection.PathSources, dx, dy);
        return WriteContentBatchAsync(sessionId, selection.PageIndex, "move_content_selection",
            document => _core.ApplyContentSelectionEdit(document, edit));
    }

    /// <summary>Removes every member of <paramref name="selection"/>.</summary>
    public Task<OperationResult<AnnotationState>> RemoveContentSelectionAsync(string sessionId, ContentSelection selection)
    {
        var edit = new PdfCoreContentSelectionEdit.Remove(selection.RunSources, selection.ImageSources, selection.PathSources);
        return WriteContentBatchAsync(sessionId, selection.PageIndex, "remove_content_selection",
            document => _core.ApplyContentSelectionEdit(document, edit));
    }
}
