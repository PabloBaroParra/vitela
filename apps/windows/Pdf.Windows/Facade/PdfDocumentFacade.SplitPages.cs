using Pdf.Windows.Localization;

namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    public async Task<OperationResult<SplitPagesPlan>> PlanSplitPagesAsync(string sessionId, string cuts)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<SplitPagesPlan>.Failure(CreateError(I18n.T("error-document-unavailable"), PdfCoreError.DocumentNotFound, "split_plan", sessionId, null));
            }

            return await Task.Run(() =>
            {
                var document = session.Document;
                OperationResult<SplitPagesPlan> Refuse(string message, PdfCoreError category) =>
                    OperationResult<SplitPagesPlan>.Failure(CreateError(message, category, "split_plan", sessionId, null));
                if (!_core.TextExtractionAllowed(document))
                    return Refuse(I18n.T("extract-forbidden"), PdfCoreError.UnsupportedOperation);
                if (!_core.FullRewriteAllowed(document))
                    return Refuse(I18n.T("split-rewrite-blocked"), PdfCoreError.UnsupportedOperation);
                if (document.PageCount < 2)
                    return Refuse(I18n.T("split-one-page"), PdfCoreError.InvalidPageSelection);

                var parts = _core.PlanSplit(cuts, document.PageCount, session.DisplayName);
                return OperationResult<SplitPagesPlan>.Success(new SplitPagesPlan(parts, _core.ExtractSourceIsSigned(document)));
            }).ConfigureAwait(false);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<SplitPagesPlan>.Failure(MapError(error, "split_plan", sessionId, null));
        }
        catch (Exception error)
        {
            return OperationResult<SplitPagesPlan>.Failure(MapUnexpected(error, "split_plan", sessionId, null));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }
}
