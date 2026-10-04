namespace Pdf.Windows.Facade;

/// <summary>
/// "Add PDFs" in the Linux shell's two phases: <see cref="PrepareImportAsync"/>
/// per picked file (no document touched), then <see cref="ImportPreparedAsync"/>
/// for the whole pick as ONE undoable edit.
/// </summary>
public sealed partial class PdfDocumentFacade
{
    public async Task<OperationResult<IImportSource>> PrepareImportAsync(byte[] bytes, string? password)
    {
        const string operation = "prepare_import";
        try { return OperationResult<IImportSource>.Success(await Task.Run(() => _core.PrepareImport(bytes, password)).ConfigureAwait(false)); }
        catch (PdfCoreException error) { return OperationResult<IImportSource>.Failure(MapImportError(error, operation, null)); }
        catch (Exception error) { return OperationResult<IImportSource>.Failure(MapUnexpected(error, operation, null, null)); }
    }

    /// <summary>The core's reason this document refuses any import, as a sentence, or <c>null</c> — asked before the picker.</summary>
    public Task<OperationResult<string?>> ImportRefusalAsync(string sessionId)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
                return Task.FromResult(OperationResult<string?>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "import_refusal", sessionId, null)));
            return Task.FromResult(OperationResult<string?>.Success(_core.ImportRefusalOf(session.Document) is { } refusal ? Sentence(refusal) : null));
        }
    }

    /// <remarks>
    /// All or nothing in the core. A preview that cannot be rebuilt afterwards
    /// is reported, but the pages are already in: as with
    /// <see cref="EditPagesAsync"/>, undo is how they leave.
    /// </remarks>
    public async Task<OperationResult<ImportedPdfs>> ImportPreparedAsync(string sessionId, IReadOnlyList<IImportSource> sources, uint index)
    {
        const string operation = "import_prepared";
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<ImportedPdfs>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, operation, sessionId, null));
            }

            PdfCoreBatchImportReport report;
            try { report = await Task.Run(() => _core.ImportPrepared(session.Document, sources, index)).ConfigureAwait(false); }
            catch (PdfCoreException error) { return OperationResult<ImportedPdfs>.Failure(MapImportError(error, operation, sessionId)); }

            lock (_gate)
            {
                session.EditRevision++;
                session.HasRecordedPreviewEdit = true;
                session.ClampPageIndex();
            }

            var refreshed = await RefreshPreviewAsync(session, operation, null).ConfigureAwait(false);
            if (!refreshed.IsSuccess) return OperationResult<ImportedPdfs>.Failure(refreshed.Error!);
            lock (_gate) return OperationResult<ImportedPdfs>.Success(new(session.ToDto(), report.PageCount, report.SourceIds));
        }
        catch (Exception error) { return OperationResult<ImportedPdfs>.Failure(MapUnexpected(error, operation, sessionId, null)); }
        finally { _documentChangeGate.Release(); }
    }

    /// <summary>An import refusal is the core's own sentence; everything else maps as usual.</summary>
    private UserSafeError MapImportError(PdfCoreException error, string operation, string? sessionId) =>
        error.Category == PdfCoreError.UnsupportedOperation && error.ReaderFacingDetail is { Length: > 0 } reason
            ? CreateError(Sentence(reason), error.Category, operation, sessionId, null)
            : MapError(error, operation, sessionId, null);
}
