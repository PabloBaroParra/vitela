namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    /// <summary>
    /// Adds every page of one PDF at <paramref name="index"/> as one undoable
    /// edit, rebuilds the preview, and hands back the new page layout.
    /// </summary>
    /// <remarks>
    /// One file per call, as <c>pdf-ffi</c>'s <c>import_pdf</c> is: a shell
    /// importing several files calls this once per file, so each is its own
    /// undo step and a password is asked about that file alone. The parse and
    /// graft run off the UI thread but inside the document-change gate, so no
    /// other page edit can land between the import and its revision bump.
    ///
    /// A preview that cannot be rebuilt is reported, but the pages are already
    /// in the document: as with <see cref="EditPagesAsync"/>, undo is how they
    /// leave, and the session says they are there.
    /// </remarks>
    public async Task<OperationResult<ImportedPdf>> ImportPdfAsync(string sessionId, byte[] bytes, string? password, uint index)
    {
        const string operation = "import_pdf";
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<ImportedPdf>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, operation, sessionId, null));
            }

            PdfCoreImportReport report;
            try
            {
                report = await Task.Run(() => _core.ImportPdf(session.Document, bytes, password, index)).ConfigureAwait(false);
            }
            catch (PdfCoreException error) when (error.Category == PdfCoreError.UnsupportedOperation && error.ReaderFacingDetail is { Length: > 0 } reason)
            {
                return OperationResult<ImportedPdf>.Failure(CreateError(Sentence(reason), error.Category, operation, sessionId, null));
            }

            lock (_gate)
            {
                session.EditRevision++;
                session.HasRecordedPreviewEdit = true;
                session.ClampPageIndex();
            }

            var refreshed = await RefreshPreviewAsync(session, operation, null).ConfigureAwait(false);
            if (!refreshed.IsSuccess) return OperationResult<ImportedPdf>.Failure(refreshed.Error!);

            lock (_gate)
            {
                return OperationResult<ImportedPdf>.Success(new(session.ToDto(), report.PageCount, report.Warnings, report.SourceId));
            }
        }
        catch (PdfCoreException error) { return OperationResult<ImportedPdf>.Failure(MapError(error, operation, sessionId, null)); }
        catch (Exception error) { return OperationResult<ImportedPdf>.Failure(MapUnexpected(error, operation, sessionId, null)); }
        finally { _documentChangeGate.Release(); }
    }
}
