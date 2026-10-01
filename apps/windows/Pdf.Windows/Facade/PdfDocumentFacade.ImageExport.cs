namespace Pdf.Windows.Facade;

/// <summary>
/// Exporting pages as images: planning which files an export writes, then
/// handing back one encoded page at a time.
/// </summary>
/// <remarks>
/// <para>
/// Every rule is the core's. The page grammar, the file names and the raster
/// ceiling come from the same functions the GTK shell calls, so the two
/// shells cannot disagree about what <c>"7-3"</c> means or what page 3 of
/// <c>report.pdf</c> is called.
/// </para>
/// <para>
/// The plan is where the reader's choices are checked, all of them, before a
/// folder is picked. An export that fails on page 300 of 400 because of a
/// choice that was wrong from the start leaves 299 files behind for nothing.
/// </para>
/// <para>
/// An annotated session is exported from its output snapshot, never from its
/// own document — see <c>SnapshotIfAnnotated</c> for why.
/// </para>
/// </remarks>
public sealed partial class PdfDocumentFacade
{
    public async Task<OperationResult<ImageExportPlan>> PlanImageExportAsync(string sessionId, ImageExportRequest request)
    {
        if (request.Dpi is < ImageExportLimits.MinDpi or > ImageExportLimits.MaxDpi)
        {
            return OperationResult<ImageExportPlan>.Failure(CreateError(
                $"Choose a resolution between {ImageExportLimits.MinDpi} and {ImageExportLimits.MaxDpi} DPI.",
                PdfCoreError.InvalidSaveRequest, "export_plan", sessionId, null));
        }

        // Behind the document gate, like the page export itself: the plan
        // reads the session's document, which a concurrent open would dispose.
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                {
                    return OperationResult<ImageExportPlan>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "export_plan", sessionId, null));
                }
            }

            return await Task.Run(() => Plan(session, request)).ConfigureAwait(false);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<ImageExportPlan>.Failure(MapError(error, "export_plan", sessionId, null));
        }
        catch (Exception error)
        {
            return OperationResult<ImageExportPlan>.Failure(MapUnexpected(error, "export_plan", sessionId, null));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    private OperationResult<ImageExportPlan> Plan(SessionEntry session, ImageExportRequest request)
    {
        var document = session.Document;
        OperationResult<ImageExportPlan> Refuse(string message, PdfCoreError category) =>
            OperationResult<ImageExportPlan>.Failure(CreateError(message, category, "export_plan", session.Id, null));

        if (!_core.TextExtractionAllowed(document))
        {
            return Refuse("This document does not permit extracting its pages as images.", PdfCoreError.UnsupportedOperation);
        }

        var total = document.PageCount;
        if (total == 0)
        {
            return Refuse("This document has no pages to export.", PdfCoreError.PageIndexOutOfBounds);
        }

        IReadOnlyList<uint> pages = request.Pages switch
        {
            ImageExportPages.All => [.. Enumerable.Range(0, (int)total).Select(page => (uint)page)],
            // Clamped rather than trusted: a page removal can leave the
            // viewer's page one past the end until it catches up.
            ImageExportPages.Current => [Math.Min(request.CurrentPage, total - 1)],
            _ => _core.ParsePageSelection(request.CustomRange, total),
        };

        if (_core.FirstPageTooLargeToExport(document, pages, request.Dpi) is { } oversized)
        {
            return Refuse($"Page {oversized + 1} is too large to export at {request.Dpi} DPI. Choose a lower resolution.", PdfCoreError.RenderFailed);
        }

        var format = CoreFormat(request.Format);
        var files = pages.Select(page => new ImageExportFile(page, _core.PageImageFileName(session.DisplayName, page, total, format))).ToList();
        return OperationResult<ImageExportPlan>.Success(new ImageExportPlan(files, request.Dpi, request.Format));
    }

    /// <summary>
    /// Readies <paramref name="sessionId"/> for an export, once, before its
    /// first <see cref="ExportPageImageAsync"/>. Pair it with
    /// <see cref="ReleaseImageExportAsync"/> when the export ends, however it
    /// ends.
    /// </summary>
    public async Task<OperationResult<bool>> PrepareImageExportAsync(string sessionId)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                {
                    return OperationResult<bool>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "export_prepare", sessionId, null));
                }
            }

            var snapshot = await Task.Run(() => SnapshotIfAnnotated(session)).ConfigureAwait(false);
            lock (_gate)
            {
                session.ExportSnapshot?.Dispose();
                session.ExportSnapshot = snapshot;
            }

            return OperationResult<bool>.Success(true);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<bool>.Failure(MapError(error, "export_prepare", sessionId, null));
        }
        catch (Exception error)
        {
            return OperationResult<bool>.Failure(MapUnexpected(error, "export_prepare", sessionId, null));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    /// <summary>
    /// Ends the export <see cref="PrepareImageExportAsync"/> started and
    /// closes its snapshot, if it made one. Behind the document gate, so no
    /// page export can still be reading it.
    /// </summary>
    public async Task ReleaseImageExportAsync(string sessionId)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            lock (_gate)
            {
                // A session replaced in the meantime closed its snapshot
                // along with itself.
                if (TryGetCurrentSession(sessionId, out var session))
                {
                    session.ExportSnapshot?.Dispose();
                    session.ExportSnapshot = null;
                }
            }
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    /// <summary>
    /// One page, rendered and encoded — the bytes of the file to write. Held
    /// behind the document gate like a save, so the session cannot be
    /// replaced and disposed while the core is still reading it.
    /// </summary>
    /// <remarks>
    /// Reads the snapshot <see cref="PrepareImageExportAsync"/> made, if any;
    /// otherwise the session's own document.
    /// </remarks>
    public async Task<OperationResult<byte[]>> ExportPageImageAsync(string sessionId, uint pageIndex, uint dpi, ImageExportFormat format)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                {
                    return OperationResult<byte[]>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "export_page", sessionId, pageIndex));
                }
            }

            var document = session.ExportDocument;
            var bytes = await Task.Run(() => _core.ExportPageImage(document, pageIndex, dpi, CoreFormat(format))).ConfigureAwait(false);
            return OperationResult<byte[]>.Success(bytes);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<byte[]>.Failure(MapError(error, "export_page", sessionId, pageIndex));
        }
        catch (Exception error)
        {
            return OperationResult<byte[]>.Failure(MapUnexpected(error, "export_page", sessionId, pageIndex));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    private static PdfCoreImageFormat CoreFormat(ImageExportFormat format) => format switch
    {
        ImageExportFormat.Png => PdfCoreImageFormat.Png,
        ImageExportFormat.Jpeg => PdfCoreImageFormat.Jpeg,
        _ => throw new ArgumentOutOfRangeException(nameof(format)),
    };
}
