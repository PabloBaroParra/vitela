namespace Pdf.Windows.Facade;

/// <summary>
/// Vector paths (B25 T-206): the loose <c>re</c>/<c>m</c>/<c>l</c> operators a
/// table border or a rule is made of. Same posture as the image commands — a
/// revision-bound snapshot in, one recorded command, then a preview refresh.
/// </summary>
/// <remarks>
/// Unlike an image, a path can be edited again before a save: the core reads
/// a pending move back at its new box, and a second command built from that
/// re-read resolves against what the first one wrote. So there is no
/// "save and reopen first" rule here — only the revision check every content
/// snapshot already has.
/// </remarks>
public sealed partial class PdfDocumentFacade
{
    private enum PathChange { Move, Bounds, Remove }

    /// <summary>Reads the page's paths with pending edits applied, serialized against document changes.</summary>
    public async Task<OperationResult<IReadOnlyList<ContentPath>>> PagePathsAsync(string sessionId, uint pageIndex)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<IReadOnlyList<ContentPath>>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "page_paths", sessionId, pageIndex));
                if (!_core.ContentEditingAllowed(session.Document))
                    return OperationResult<IReadOnlyList<ContentPath>>.Failure(CreateError("This document does not permit content changes.", PdfCoreError.UnsupportedOperation, "page_paths", sessionId, pageIndex));
            }

            var paths = await Task.Run(() => _core.ReadPagePaths(session.Document, pageIndex)).ConfigureAwait(false);
            return OperationResult<IReadOnlyList<ContentPath>>.Success(
                [.. paths.Select(path => new ContentPath(path, sessionId, session.EditRevision))]);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<IReadOnlyList<ContentPath>>.Failure(MapError(error, "page_paths", sessionId, pageIndex));
        }
        catch (Exception error)
        {
            return OperationResult<IReadOnlyList<ContentPath>>.Failure(MapUnexpected(error, "page_paths", sessionId, pageIndex));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    /// <summary>Moves one path in PDF space without changing its size.</summary>
    public Task<OperationResult<AnnotationState>> MovePathAsync(string sessionId, ContentPath path, double x, double y) =>
        ChangePathAsync(sessionId, path, PathChange.Move, new PdfCoreRect(x, y, path.Bounds.Width, path.Bounds.Height));

    /// <summary>
    /// Reshapes one path to <paramref name="bounds"/>, for a corner drag. The
    /// core rewrites the coordinates, so the line width does not change.
    /// </summary>
    public Task<OperationResult<AnnotationState>> SetPathBoundsAsync(string sessionId, ContentPath path, AnnotationRect bounds) =>
        ChangePathAsync(sessionId, path, PathChange.Bounds, new PdfCoreRect(bounds.X, bounds.Y, bounds.Width, bounds.Height));

    /// <summary>Removes one path's operators from the page.</summary>
    public Task<OperationResult<AnnotationState>> RemovePathAsync(string sessionId, ContentPath path) =>
        ChangePathAsync(sessionId, path, PathChange.Remove, path.Source.Bbox);

    private async Task<OperationResult<AnnotationState>> ChangePathAsync(string sessionId, ContentPath path, PathChange change, PdfCoreRect target)
    {
        var operation = change switch
        {
            PathChange.Move => "move_path",
            PathChange.Bounds => "resize_path",
            _ => "remove_path",
        };
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, operation, sessionId, path.PageIndex));
                if (path.SessionId != sessionId || path.Revision != session.EditRevision)
                    return OperationResult<AnnotationState>.Failure(CreateError("The document changed. Please try again.", PdfCoreError.UnsupportedOperation, operation, sessionId, path.PageIndex));
                if (!_core.ContentEditingAllowed(session.Document))
                    return OperationResult<AnnotationState>.Failure(CreateError("This document does not permit content changes.", PdfCoreError.UnsupportedOperation, operation, sessionId, path.PageIndex));
                // A rule is flat, so a zero extent is legal; a negative one is a
                // drag that crossed its own corner and means nothing to the core.
                if (!double.IsFinite(target.X) || !double.IsFinite(target.Y) || !double.IsFinite(target.Width)
                    || !double.IsFinite(target.Height) || target.Width < 0 || target.Height < 0)
                    return OperationResult<AnnotationState>.Failure(CreateError("Shape bounds must be finite and not negative.", PdfCoreError.UnsupportedOperation, operation, sessionId, path.PageIndex));
                if (change != PathChange.Remove && path.Source.Bbox == target)
                    return OperationResult<AnnotationState>.Success(session.AnnotationState(_core));

                _core.ApplyEdit(session.Document, change switch
                {
                    PathChange.Move => new PdfCoreEdit.MovePath(path.Source, target),
                    PathChange.Bounds => new PdfCoreEdit.ResizePath(path.Source, target),
                    _ => new PdfCoreEdit.RemovePath(path.Source),
                });
                session.EditRevision++;
                session.HasRecordedPreviewEdit = true;
            }

            return await RefreshPreviewAsync(session, operation, path.PageIndex).ConfigureAwait(false);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<AnnotationState>.Failure(MapError(error, operation, sessionId, path.PageIndex));
        }
        catch (Exception error)
        {
            return OperationResult<AnnotationState>.Failure(MapUnexpected(error, operation, sessionId, path.PageIndex));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }
}
