namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    internal const string TextExtractionRefusalMessage = "This document does not permit copying or extracting its text.";
    public Task<SearchResult> SearchAsync(string sessionId, string query)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
                return Task.FromResult(SearchResult.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "search", sessionId, null)));
            try
            {
                // Query the core's extraction permission, never annotation/content editing.
                // No matcher work or match count may escape a refusal.
                if (!_core.TextExtractionAllowed(session.Document))
                    return Task.FromResult(SearchResult.Failure(CreateError(TextExtractionRefusalMessage, PdfCoreError.UnsupportedOperation, "search", sessionId, null)));
                return QueueSearchLocked(session, query);
            }
            catch (PdfCoreException error)
            {
                return Task.FromResult(SearchResult.Failure(MapError(error, "search", sessionId, null)));
            }
            catch (Exception error)
            {
                return Task.FromResult(SearchResult.Failure(MapUnexpected(error, "search", sessionId, null)));
            }
        }
    }
}
