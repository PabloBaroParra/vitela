namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    private enum ImageChange { Resize, Move, Remove }

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
    public Task<OperationResult<AnnotationState>> ResizeImageAsync(string sessionId, ContentImage image, double width, double height) =>
        ChangeImageAsync(sessionId, image, ImageChange.Resize, width, height);

    /// <summary>Moves one image in PDF space without changing its dimensions.</summary>
    public Task<OperationResult<AnnotationState>> MoveImageAsync(string sessionId, ContentImage image, double x, double y) =>
        ChangeImageAsync(sessionId, image, ImageChange.Move, x, y);

    /// <summary>Removes an existing image, retaining its original source for undo in the core.</summary>
    public Task<OperationResult<AnnotationState>> RemoveImageAsync(string sessionId, ContentImage image) =>
        ChangeImageAsync(sessionId, image, ImageChange.Remove);

    private async Task<OperationResult<AnnotationState>> ChangeImageAsync(string sessionId, ContentImage image, ImageChange change, double first = 0, double second = 0)
    {
        var move = change == ImageChange.Move;
        var remove = change == ImageChange.Remove;
        var operation = remove ? "remove_image" : move ? "move_image" : "resize_image";
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, operation, sessionId, image.PageIndex));
                if (image.SessionId != sessionId || image.Revision != session.EditRevision)
                    return OperationResult<AnnotationState>.Failure(CreateError("The document changed. Please try again.", PdfCoreError.UnsupportedOperation, operation, sessionId, image.PageIndex));
                if (!_core.ContentEditingAllowed(session.Document))
                    return OperationResult<AnnotationState>.Failure(CreateError("This document does not permit content changes.", PdfCoreError.UnsupportedOperation, operation, sessionId, image.PageIndex));
                if (!remove && (!double.IsFinite(first) || !double.IsFinite(second) || (!move && (first <= 0 || second <= 0))))
                    return OperationResult<AnnotationState>.Failure(CreateError(move ? "Image coordinates must be finite." : "Image dimensions must be finite and greater than zero.", PdfCoreError.UnsupportedOperation, operation, sessionId, image.PageIndex));
                var bounds = image.Source.Bbox;
                var target = move ? bounds with { X = first, Y = second } : bounds with { Width = first, Height = second };
                if (!remove && bounds == target)
                    return OperationResult<AnnotationState>.Success(session.AnnotationState(_core));

                _core.ApplyEdit(session.Document, remove ? new PdfCoreEdit.RemoveImage(image.Source)
                    : move ? new PdfCoreEdit.MoveImage(image.Source, target) : new PdfCoreEdit.ResizeImage(image.Source, target));
                session.EditRevision++;
                session.HasRecordedPreviewEdit = true;
            }

            return await RefreshPreviewAsync(session, operation, image.PageIndex).ConfigureAwait(false);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<AnnotationState>.Failure(MapError(error, operation, sessionId, image.PageIndex));
        }
        catch (Exception error)
        {
            return OperationResult<AnnotationState>.Failure(MapUnexpected(error, operation, sessionId, image.PageIndex));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }
}
