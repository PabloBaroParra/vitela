using Pdf.Windows.Localization;

namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    /// <summary>Asks the core whether the image can be replaced before a picker opens.</summary>
    public Task<OperationResult<AnnotationState>> PrepareImageReplacementAsync(string sessionId, ContentImage image) =>
        ReplaceImageSourceAsync(sessionId, image, null);

    public Task<OperationResult<AnnotationState>> ReplaceContentImageAsync(string sessionId, ContentImage image, byte[] bytes) =>
        ReplaceImageSourceAsync(sessionId, image, bytes.ToArray());

    private async Task<OperationResult<AnnotationState>> ReplaceImageSourceAsync(string sessionId, ContentImage image, byte[]? after)
    {
        const string operation = "replace_content_image";
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<AnnotationState>.Failure(CreateError(I18n.T("error-document-unavailable"), PdfCoreError.DocumentNotFound, operation, sessionId, image.PageIndex));
                if (image.SessionId != sessionId || image.Revision != session.EditRevision)
                    return OperationResult<AnnotationState>.Failure(CreateError(I18n.T("error-document-changed-reopen"), PdfCoreError.UnsupportedOperation, operation, sessionId, image.PageIndex));
                if (!_core.ContentEditingAllowed(session.Document))
                    return OperationResult<AnnotationState>.Failure(CreateError(I18n.T("refusal-content-forbidden"), PdfCoreError.UnsupportedOperation, operation, sessionId, image.PageIndex));
            }
            // Checked against the original page backing, not the rendered preview. The
            // core refuses an image already carrying a pending edit only where it may
            // not checkpoint; elsewhere it writes the queue into a new base and
            // validates there. Every encoding is replaceable, because undo restores
            // the original exactly.
            await Task.Run(() => _core.CheckImageReplacement(session.Document, image.Source)).ConfigureAwait(false);
            lock (_gate)
            {
                if (after is null) return OperationResult<AnnotationState>.Success(session.AnnotationState(_core));
                _core.ApplyEdit(session.Document, new PdfCoreEdit.ReplaceImageSource(image.Source, after));
                session.EditRevision++;
                session.HasRecordedPreviewEdit = true;
            }
            return await RefreshPreviewAsync(session, operation, image.PageIndex).ConfigureAwait(false);
        }
        catch (PdfCoreException error) { return OperationResult<AnnotationState>.Failure(MapError(error, operation, sessionId, image.PageIndex)); }
        catch (Exception error) { return OperationResult<AnnotationState>.Failure(MapUnexpected(error, operation, sessionId, image.PageIndex)); }
        finally { _documentChangeGate.Release(); }
    }
}
