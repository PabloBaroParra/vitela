namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    /// <summary>Reads images with pending edits applied, serialized against document changes.</summary>
    public async Task<OperationResult<IReadOnlyList<ContentImage>>> PageImagesAsync(string sessionId, uint pageIndex)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<IReadOnlyList<ContentImage>>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "page_images", sessionId, pageIndex));
                if (!_core.ContentEditingAllowed(session.Document))
                    return OperationResult<IReadOnlyList<ContentImage>>.Failure(CreateError("This document does not permit content changes.", PdfCoreError.UnsupportedOperation, "page_images", sessionId, pageIndex));
            }

            var content = await Task.Run(() => _core.ReadPageContent(session.Document, pageIndex)).ConfigureAwait(false);
            return OperationResult<IReadOnlyList<ContentImage>>.Success(
                [.. content.Images.Select(image => new ContentImage(image, sessionId, session.EditRevision))]);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<IReadOnlyList<ContentImage>>.Failure(MapError(error, "page_images", sessionId, pageIndex));
        }
        catch (Exception error)
        {
            return OperationResult<IReadOnlyList<ContentImage>>.Failure(MapUnexpected(error, "page_images", sessionId, pageIndex));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    /// <summary>Resizes one image at its existing origin, then refreshes the PDF preview.</summary>
    public async Task<OperationResult<AnnotationState>> ResizeImageAsync(string sessionId, ContentImage image, double width, double height)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "resize_image", sessionId, image.PageIndex));
                if (image.SessionId != sessionId || image.Revision != session.EditRevision)
                    return OperationResult<AnnotationState>.Failure(CreateError("The document changed. Please try again.", PdfCoreError.UnsupportedOperation, "resize_image", sessionId, image.PageIndex));
                if (!_core.ContentEditingAllowed(session.Document))
                    return OperationResult<AnnotationState>.Failure(CreateError("This document does not permit content changes.", PdfCoreError.UnsupportedOperation, "resize_image", sessionId, image.PageIndex));
                if (!double.IsFinite(width) || !double.IsFinite(height) || width <= 0 || height <= 0)
                    return OperationResult<AnnotationState>.Failure(CreateError("Image dimensions must be finite and greater than zero.", PdfCoreError.UnsupportedOperation, "resize_image", sessionId, image.PageIndex));
                var bounds = image.Source.Bbox;
                if (bounds.Width == width && bounds.Height == height)
                    return OperationResult<AnnotationState>.Success(session.AnnotationState(_core));

                _core.ApplyEdit(session.Document, new PdfCoreEdit.ResizeImage(image.Source, bounds with { Width = width, Height = height }));
                session.EditRevision++;
                session.HasRecordedPreviewEdit = true;
            }

            return await RefreshPreviewAsync(session, "resize_image", image.PageIndex).ConfigureAwait(false);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<AnnotationState>.Failure(MapError(error, "resize_image", sessionId, image.PageIndex));
        }
        catch (Exception error)
        {
            return OperationResult<AnnotationState>.Failure(MapUnexpected(error, "resize_image", sessionId, image.PageIndex));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }
}
