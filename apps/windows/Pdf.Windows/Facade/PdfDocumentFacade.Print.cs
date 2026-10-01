namespace Pdf.Windows.Facade;

/// <summary>
/// Printing: preparing what a print job rasterizes, and rendering it one page
/// at a time outside the viewer's coalesced render queue.
/// </summary>
/// <remarks>
/// A print job renders an annotated session from its output snapshot, never
/// from the session's own document — see <c>SnapshotIfAnnotated</c> for why.
/// </remarks>
public sealed partial class PdfDocumentFacade
{
    /// <summary>
    /// Readies <paramref name="sessionId"/> for a print job and returns how
    /// many pages it will print. Call once per job, before the first
    /// <see cref="RenderPageForPrintAsync"/>, and pair it with
    /// <see cref="ReleasePrint"/> when the job ends.
    /// </summary>
    /// <remarks>
    /// A session without annotations prints its own document: there is
    /// nothing the preview leaves out, and no save that could fail.
    /// </remarks>
    public async Task<OperationResult<uint>> PreparePrintAsync(string sessionId)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                {
                    return OperationResult<uint>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "print_prepare", sessionId, null));
                }
            }

            var snapshot = await Task.Run(() => SnapshotIfAnnotated(session)).ConfigureAwait(false);

            lock (_gate)
            {
                if (session.Retired)
                {
                    snapshot?.Dispose();
                    return OperationResult<uint>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "print_prepare", sessionId, null));
                }

                ReleasePrintLocked(session);
                session.PrintSnapshot = snapshot;
                return OperationResult<uint>.Success(session.PrintDocument.PageCount);
            }
        }
        catch (PdfCoreException error)
        {
            return OperationResult<uint>.Failure(MapError(error, "print_prepare", sessionId, null));
        }
        catch (Exception error)
        {
            return OperationResult<uint>.Failure(MapUnexpected(error, "print_prepare", sessionId, null));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    /// <summary>
    /// Ends the print job <see cref="PreparePrintAsync"/> started. Its
    /// snapshot is closed once no page render is still reading it.
    /// </summary>
    public void ReleasePrint(string sessionId)
    {
        lock (_gate)
        {
            if (TryGetCurrentSession(sessionId, out var session))
            {
                ReleasePrintLocked(session);
            }
        }
    }

    /// <summary>
    /// Renders one page at print quality, deliberately independent of the
    /// coalesced viewer render queue so a scrolling page cannot supersede it.
    /// Callers render a document one page at a time and release each page's
    /// pixels before requesting the next, so printing a large document never
    /// holds every page's raw bitmap in memory at once.
    /// </summary>
    /// <remarks>
    /// Reads the snapshot <see cref="PreparePrintAsync"/> made, if any;
    /// otherwise the session's own document.
    /// </remarks>
    public Task<RenderResult> RenderPageForPrintAsync(string sessionId, uint pageIndex, uint dpi, bool invertContentColors)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
            {
                return Task.FromResult(RenderResult.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "print_render", sessionId, pageIndex)));
            }

            var document = session.PrintDocument;
            if (pageIndex >= document.PageCount)
            {
                return Task.FromResult(RenderResult.Failure(CreateError("The document changed. Please try again.", PdfCoreError.PageIndexOutOfBounds, "print_render", sessionId, pageIndex)));
            }

            session.InFlightPrints++;
            return RenderPageForPrintAsync(session, document, pageIndex, dpi, invertContentColors);
        }
    }

    private static void ReleasePrintLocked(SessionEntry session)
    {
        if (session.PrintSnapshot is { } snapshot)
        {
            session.PrintSnapshot = null;
            session.ReleasedPrintSnapshots.Add(snapshot);
        }

        session.DisposeReleasedPrintSnapshotsIfIdle();
    }

    private async Task<RenderResult> RenderPageForPrintAsync(SessionEntry session, IPdfCoreDocument document, uint pageIndex, uint dpi, bool invertContentColors)
    {
        RenderResult result;
        try
        {
            var bitmap = await Task.Run(() => _core.RenderPage(document, pageIndex, dpi, invertContentColors)).ConfigureAwait(false);
            result = RenderResult.Success(new RenderedPage(session.Id, pageIndex, (ulong)pageIndex + 1, bitmap.Width, bitmap.Height, bitmap.Stride, bitmap.Rgba));
        }
        catch (PdfCoreException error)
        {
            result = RenderResult.Failure(MapError(error, "print_render", session.Id, pageIndex));
        }
        catch (Exception error)
        {
            result = RenderResult.Failure(MapUnexpected(error, "print_render", session.Id, pageIndex));
        }

        lock (_gate)
        {
            session.InFlightPrints--;
            if (session.Retired)
            {
                if (session.HasNoInFlightOperations)
                {
                    session.Dispose();
                }

                return RenderResult.Discarded();
            }

            session.DisposeReleasedPrintSnapshotsIfIdle();
            return _currentSession == session ? result : RenderResult.Discarded();
        }
    }
}
