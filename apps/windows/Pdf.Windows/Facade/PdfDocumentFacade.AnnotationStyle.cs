namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    /// <summary>Commit a color only if the annotation captured by the dialog still matches.</summary>
    public async Task<OperationResult<AnnotationState>> RestyleAnnotationAsync(string sessionId, Annotation before, AnnotationColor color)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out var session))
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "annotation_style", sessionId, null));
                try
                {
                    var state = session.AnnotationState(_core);
                    var current = state.Annotations.SingleOrDefault(a => a.Id == before.Id);
                    if (current is null || (current with { Points = before.Points }) != before
                        || !current.Points.SequenceEqual(before.Points))
                        return OperationResult<AnnotationState>.Failure(CreateError("Color selection is no longer current.", PdfCoreError.UnsupportedOperation, "annotation_style", sessionId, before.PageIndex));
                    if (!state.EditingAllowed)
                        return OperationResult<AnnotationState>.Failure(CreateError("This document does not permit editing annotations.", PdfCoreError.UnsupportedOperation, "annotation_style", sessionId, before.PageIndex));
                    if (current.Color == color) return OperationResult<AnnotationState>.Success(state);
                    _core.ApplyEdit(session.Document, new PdfCoreEdit.Restyle(before.Id, new PdfCoreColor(color.R, color.G, color.B)));
                    session.EditRevision++;
                    return OperationResult<AnnotationState>.Success(session.AnnotationState(_core));
                }
                catch (PdfCoreException error)
                {
                    return OperationResult<AnnotationState>.Failure(MapError(error, "annotation_style", sessionId, before.PageIndex));
                }
            }
        }
        finally { _documentChangeGate.Release(); }
    }
}
