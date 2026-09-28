namespace Pdf.Windows.Facade;

/// <summary>
/// Extract: pulling a subset of the open document's pages out into a new
/// PDF.
/// </summary>
/// <remarks>
/// <para>
/// The Windows twin of the Linux shell's <c>write::extract</c> chain
/// (<c>apps/linux-gtk/src/app/write/extract/</c>) and the closest relative of
/// <see cref="PdfDocumentFacade.PlanImageExportAsync"/>'s chain here: both
/// plan against the core's own grammar and permission answers before
/// anything is written, and both leave the open session exactly as it was —
/// an extraction writes a second file, never the one the reader is looking
/// at.
/// </para>
/// <para>
/// Both gates are asked in the plan, before a destination is ever picked:
/// <see cref="IPdfCore.TextExtractionAllowed"/> (<c>/P</c> bit 5 — the same
/// bit image export asks) and <see cref="IPdfCore.FullRewriteAllowed"/> (a
/// new page set can only come from a full rewrite, which an encrypted
/// document opened with only one of its two passwords can never produce).
/// </para>
/// </remarks>
public sealed partial class PdfDocumentFacade
{
    public async Task<OperationResult<ExtractPagesPlan>> PlanExtractPagesAsync(string sessionId, ExtractPagesRequest request)
    {
        // Behind the document gate, like the image-export plan: this reads
        // the session's document, which a concurrent open would dispose.
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                {
                    return OperationResult<ExtractPagesPlan>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "extract_plan", sessionId, null));
                }
            }

            return await Task.Run(() => Plan(session, request)).ConfigureAwait(false);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<ExtractPagesPlan>.Failure(MapError(error, "extract_plan", sessionId, null));
        }
        catch (Exception error)
        {
            return OperationResult<ExtractPagesPlan>.Failure(MapUnexpected(error, "extract_plan", sessionId, null));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    private OperationResult<ExtractPagesPlan> Plan(SessionEntry session, ExtractPagesRequest request)
    {
        var document = session.Document;
        OperationResult<ExtractPagesPlan> Refuse(string message, PdfCoreError category) =>
            OperationResult<ExtractPagesPlan>.Failure(CreateError(message, category, "extract_plan", session.Id, null));

        // Deliberately not the page-assembly permission: an extraction
        // changes nothing about the open document, so the bit that governs
        // *its* pages is not the question. See `pdf-ffi`'s `extract` module
        // for the full rationale, shared verbatim by both shells.
        if (!_core.TextExtractionAllowed(document))
        {
            return Refuse("This document does not permit extracting its pages.", PdfCoreError.UnsupportedOperation);
        }
        if (!_core.FullRewriteAllowed(document))
        {
            return Refuse(
                "Extracting pages rewrites the whole file, which this document's encryption or available credentials do not allow.",
                PdfCoreError.UnsupportedOperation);
        }

        var total = document.PageCount;
        if (total == 0)
        {
            return Refuse("This document has no pages to extract.", PdfCoreError.PageIndexOutOfBounds);
        }
        if (string.IsNullOrWhiteSpace(request.PageRange))
        {
            return Refuse("Type which pages to extract, for example 1-3,7.", PdfCoreError.InvalidPageSelection);
        }

        IReadOnlyList<uint> pages;
        try
        {
            pages = _core.ParsePageSelection(request.PageRange, total);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<ExtractPagesPlan>.Failure(MapError(error, "extract_plan", session.Id, null));
        }

        return OperationResult<ExtractPagesPlan>.Success(new ExtractPagesPlan(pages, _core.ExtractSourceIsSigned(document)));
    }

    /// <summary>
    /// Prunes a clone of the session's document down to <paramref name="pages"/>
    /// and returns the bytes of the resulting PDF. The live document — and the
    /// session the reader is looking at — are both untouched.
    /// </summary>
    public async Task<OperationResult<byte[]>> ExtractPagesAsync(string sessionId, IReadOnlyList<uint> pages)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                {
                    return OperationResult<byte[]>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "extract", sessionId, null));
                }
            }

            var bytes = await Task.Run(() => _core.ExtractPagesToPdf(session.Document, pages)).ConfigureAwait(false);
            return OperationResult<byte[]>.Success(bytes);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<byte[]>.Failure(MapError(error, "extract", sessionId, null));
        }
        catch (Exception error)
        {
            return OperationResult<byte[]>.Failure(MapUnexpected(error, "extract", sessionId, null));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }
}
