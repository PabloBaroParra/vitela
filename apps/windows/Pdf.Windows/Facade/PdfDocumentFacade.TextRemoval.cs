namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    /// <summary>Reads revision-bound text targets for deletion, serialized against document changes.</summary>
    public async Task<OperationResult<IReadOnlyList<ContentTextRun>>> PageTextDeletionTargetsAsync(string sessionId, uint pageIndex)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<IReadOnlyList<ContentTextRun>>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "text_deletion_targets", sessionId, pageIndex));
                if (!_core.ContentEditingAllowed(session.Document))
                    return OperationResult<IReadOnlyList<ContentTextRun>>.Failure(CreateError("This document does not permit content changes.", PdfCoreError.UnsupportedOperation, "text_deletion_targets", sessionId, pageIndex));
            }
            var content = await Task.Run(() => _core.ReadPageContent(session.Document, pageIndex)).ConfigureAwait(false);
            return OperationResult<IReadOnlyList<ContentTextRun>>.Success(
                [.. content.TextRuns.Select(run => new ContentTextRun(run, null, sessionId, session.EditRevision))]);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<IReadOnlyList<ContentTextRun>>.Failure(MapError(error, "text_deletion_targets", sessionId, pageIndex));
        }
        catch (Exception error)
        {
            return OperationResult<IReadOnlyList<ContentTextRun>>.Failure(MapUnexpected(error, "text_deletion_targets", sessionId, pageIndex));
        }
        finally { _documentChangeGate.Release(); }
    }

    /// <summary>Removes one text run without replacing its font, then refreshes the PDF preview.</summary>
    public async Task<OperationResult<AnnotationState>> RemoveTextRunAsync(string sessionId, ContentTextRun run)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "remove_text", sessionId, run.PageIndex));
                if (run.SessionId != sessionId || run.Revision != session.EditRevision)
                    return OperationResult<AnnotationState>.Failure(CreateError("The document changed. Please reopen the dialog.", PdfCoreError.UnsupportedOperation, "remove_text", sessionId, run.PageIndex));
                if (!_core.ContentEditingAllowed(session.Document))
                    return OperationResult<AnnotationState>.Failure(CreateError("This document does not permit content changes.", PdfCoreError.UnsupportedOperation, "remove_text", sessionId, run.PageIndex));
                _core.ApplyEdit(session.Document, new PdfCoreEdit.RemoveTextRun(run.Source));
                session.EditRevision++;
                session.HasRecordedPreviewEdit = true;
            }
            return await RefreshPreviewAsync(session, "remove_text", run.PageIndex).ConfigureAwait(false);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<AnnotationState>.Failure(MapError(error, "remove_text", sessionId, run.PageIndex));
        }
        catch (Exception error)
        {
            return OperationResult<AnnotationState>.Failure(MapUnexpected(error, "remove_text", sessionId, run.PageIndex));
        }
        finally { _documentChangeGate.Release(); }
    }
}
