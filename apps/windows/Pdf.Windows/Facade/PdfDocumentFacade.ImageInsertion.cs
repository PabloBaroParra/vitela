namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    public async Task<OperationResult<ImageInsertionTarget>> PrepareImageInsertionAsync(string sessionId, uint pageIndex)
    {
        const string operation = "prepare_image_insertion";
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out var session))
                    return OperationResult<ImageInsertionTarget>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, operation, sessionId, pageIndex));
                if (!_core.ContentEditingAllowed(session.Document))
                    return OperationResult<ImageInsertionTarget>.Failure(CreateError("This document does not permit content changes.", PdfCoreError.UnsupportedOperation, operation, sessionId, pageIndex));
                if (pageIndex >= session.Document.PageCount)
                    return OperationResult<ImageInsertionTarget>.Failure(CreateError("The page is no longer available.", PdfCoreError.PageIndexOutOfBounds, operation, sessionId, pageIndex));
                return OperationResult<ImageInsertionTarget>.Success(new(sessionId, session.EditRevision, pageIndex));
            }
        }
        catch (PdfCoreException error) { return OperationResult<ImageInsertionTarget>.Failure(MapError(error, operation, sessionId, pageIndex)); }
        catch (Exception error) { return OperationResult<ImageInsertionTarget>.Failure(MapUnexpected(error, operation, sessionId, pageIndex)); }
        finally { _documentChangeGate.Release(); }
    }

    /// <summary>Inserts page content using the core's proportional, top-left anchored placement.</summary>
    public async Task<OperationResult<AnnotationState>> InsertContentImageAsync(string sessionId, ImageInsertionTarget target, byte[] bytes, double x, double y)
    {
        const string operation = "insert_content_image";
        // Own the bytes before yielding; callers must not mutate a queued edit's source.
        var source = bytes.ToArray();
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, operation, sessionId, target.PageIndex));
                if (target.SessionId != sessionId || target.Revision != session.EditRevision)
                    return OperationResult<AnnotationState>.Failure(CreateError("The document changed. Please reopen the dialog.", PdfCoreError.UnsupportedOperation, operation, sessionId, target.PageIndex));
                if (!_core.ContentEditingAllowed(session.Document))
                    return OperationResult<AnnotationState>.Failure(CreateError("This document does not permit content changes.", PdfCoreError.UnsupportedOperation, operation, sessionId, target.PageIndex));
                if (!double.IsFinite(x) || !double.IsFinite(y))
                    return OperationResult<AnnotationState>.Failure(CreateError("Image coordinates must be finite.", PdfCoreError.UnsupportedOperation, operation, sessionId, target.PageIndex));
            }
            var bounds = await Task.Run(() => _core.StampPlacement(source, x, y)).ConfigureAwait(false);
            lock (_gate)
            {
                // A fresh resource also separates multiple pending insertions. The core
                // refuses an existing name rather than overwriting any page resource.
                _core.ApplyEdit(session.Document, new PdfCoreEdit.InsertImage(new(
                    0, target.PageIndex, bounds, "VitelaImage" + Guid.NewGuid().ToString("N")), source));
                session.EditRevision++;
                session.HasRecordedPreviewEdit = true;
            }
            return await RefreshPreviewAsync(session, operation, target.PageIndex).ConfigureAwait(false);
        }
        catch (PdfCoreException error) { return OperationResult<AnnotationState>.Failure(MapError(error, operation, sessionId, target.PageIndex)); }
        catch (Exception error) { return OperationResult<AnnotationState>.Failure(MapUnexpected(error, operation, sessionId, target.PageIndex)); }
        finally { _documentChangeGate.Release(); }
    }
}
