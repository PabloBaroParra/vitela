namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    /// <summary>
    /// Deletes a field, its widgets with it, as one undoable core command. Same
    /// structural permission as creating one; refuses a row whose name no
    /// longer matches, so a stale click never removes a different field.
    /// </summary>
    public async Task<OperationResult<AnnotationState>> RemoveFormFieldAsync(string sessionId, ulong fieldId, string expectedName)
    {
        const string operation = "form_remove";
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
                    return OperationResult<AnnotationState>.Failure(CreateError("This document does not permit deleting form fields.", PdfCoreError.UnsupportedOperation, operation, sessionId, null));
                try
                {
                    var field = _core.ListFormFields(session.Document).FirstOrDefault(candidate => candidate.Id == fieldId);
                    if (field is null || field.Name != expectedName)
                        return OperationResult<AnnotationState>.Failure(CreateError("The document changed. Please try again.", PdfCoreError.FormFieldNotFound, operation, sessionId, null));
                    pageIndex = field.PageIndex;
                    _core.ApplyEdit(session.Document, new PdfCoreEdit.RemoveFormField(fieldId));
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
