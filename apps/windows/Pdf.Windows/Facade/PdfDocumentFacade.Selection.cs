namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    /// <summary>Rechecks the extraction boundary before selecting or copying cached characters.</summary>
    public OperationResult<bool> SelectionAllowed(string sessionId)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
                return OperationResult<bool>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "selection", sessionId, null));
            try
            {
                return _core.TextExtractionAllowed(session.Document)
                    ? OperationResult<bool>.Success(true)
                    : OperationResult<bool>.Failure(CreateError(TextExtractionRefusalMessage, PdfCoreError.UnsupportedOperation, "selection", sessionId, null));
            }
            catch (PdfCoreException error) { return OperationResult<bool>.Failure(MapError(error, "selection", sessionId, null)); }
            catch (Exception error) { return OperationResult<bool>.Failure(MapUnexpected(error, "selection", sessionId, null)); }
        }
    }

    /// <summary>Loads characters under the document-change gate; no handle outlives a changed preview.</summary>
    public async Task<OperationResult<PageCharacters>> PageCharactersAsync(string sessionId, uint pageIndex)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<PageCharacters>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "page_characters", sessionId, pageIndex));
                if (!_core.TextExtractionAllowed(session.Document))
                    return OperationResult<PageCharacters>.Failure(CreateError(TextExtractionRefusalMessage, PdfCoreError.UnsupportedOperation, "page_characters", sessionId, pageIndex));
            }
            var handle = await Task.Run(() => _core.PageCharacters(session.Document, pageIndex)).ConfigureAwait(false);
            lock (_gate)
            {
                if (session.Retired || _currentSession != session)
                {
                    handle.Dispose();
                    return OperationResult<PageCharacters>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "page_characters", sessionId, pageIndex));
                }
                return OperationResult<PageCharacters>.Success(new PageCharacters(pageIndex, handle));
            }
        }
        catch (PdfCoreException error) { return OperationResult<PageCharacters>.Failure(MapError(error, "page_characters", sessionId, pageIndex)); }
        catch (Exception error) { return OperationResult<PageCharacters>.Failure(MapUnexpected(error, "page_characters", sessionId, pageIndex)); }
        finally { _documentChangeGate.Release(); }
    }
}
