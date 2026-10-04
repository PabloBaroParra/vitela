namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    /// <summary>A corner drag changes origin and size as one guarded, undoable core command.</summary>
    public async Task<OperationResult<AnnotationState>> ResizeFormFieldAsync(string sessionId, ulong fieldId,
        AnnotationRect expected, AnnotationRect replacement)
    {
        const string operation = "form_resize";
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            uint pageIndex;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, operation, sessionId, null));
                if (!_core.FormFieldEditingAllowed(session.Document))
                    return OperationResult<AnnotationState>.Failure(CreateError("This document does not permit resizing form fields.", PdfCoreError.UnsupportedOperation, operation, sessionId, null));
                if (!double.IsFinite(replacement.X) || !double.IsFinite(replacement.Y) || !double.IsFinite(replacement.Width)
                    || !double.IsFinite(replacement.Height) || replacement.Width <= 0 || replacement.Height <= 0)
                    return OperationResult<AnnotationState>.Failure(CreateError("Enter finite, positive field dimensions.", PdfCoreError.UnsupportedOperation, operation, sessionId, null));
                try
                {
                    var field = _core.ListFormFields(session.Document).FirstOrDefault(candidate => candidate.Id == fieldId);
                    if (field?.Rect is not { } rect || new AnnotationRect(rect.X, rect.Y, rect.Width, rect.Height) != expected)
                        return OperationResult<AnnotationState>.Failure(CreateError("The document changed. Please try again.", PdfCoreError.FormFieldNotFound, operation, sessionId, null));
                    if (expected == replacement) return OperationResult<AnnotationState>.Success(session.AnnotationState(_core));
                    pageIndex = field.PageIndex;
                    _core.ApplyEdit(session.Document, new PdfCoreEdit.ResizeFormField(fieldId,
                        new PdfCoreRect(replacement.X, replacement.Y, replacement.Width, replacement.Height)));
                    session.EditRevision++;
                    session.HasRecordedPreviewEdit = true;
                }
                catch (PdfCoreException error)
                {
                    return OperationResult<AnnotationState>.Failure(MapError(error, operation, sessionId, null));
                }
            }
            return await RefreshPreviewAsync(session, operation, pageIndex).ConfigureAwait(false);
        }
        finally { _documentChangeGate.Release(); }
    }
}
