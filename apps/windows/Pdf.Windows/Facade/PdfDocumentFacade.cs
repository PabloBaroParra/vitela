namespace Pdf.Windows.Facade;

public sealed class PdfDocumentFacade : IDisposable
{
    private readonly IPdfCore _core;
    private readonly IDiagnosticLogger _diagnostics;
    private readonly object _gate = new();
    private SessionEntry? _currentSession;

    internal PdfDocumentFacade(IPdfCore core, IDiagnosticLogger diagnostics)
    {
        _core = core;
        _diagnostics = diagnostics;
    }

    public async Task<OperationResult<DocumentSession>> OpenAsync(DocumentSource source, string? password = null)
    {
        try
        {
            var document = await Task.Run(() => _core.OpenFromBytes(source.Bytes, password)).ConfigureAwait(false);
            var session = new SessionEntry(Guid.NewGuid().ToString("N"), source.DisplayName, document);
            lock (_gate)
            {
                RetireCurrentSessionLocked();
                _currentSession = session;
            }

            return OperationResult<DocumentSession>.Success(session.ToDto());
        }
        catch (PdfCoreException error)
        {
            return OperationResult<DocumentSession>.Failure(MapError(error, "open", null, null));
        }
        catch (Exception error)
        {
            return OperationResult<DocumentSession>.Failure(MapUnexpected(error, "open", null, null));
        }
    }

    public OperationResult<DocumentSession> OpenReadFailure(Exception error)
    {
        return OperationResult<DocumentSession>.Failure(MapUnexpected(error, "open", null, null));
    }

    public async Task<OperationResult<DocumentSession>> CreateBlankAsync()
    {
        try
        {
            var document = await Task.Run(_core.CreateBlank).ConfigureAwait(false);
            var session = new SessionEntry(Guid.NewGuid().ToString("N"), "Untitled", document);
            lock (_gate)
            {
                RetireCurrentSessionLocked();
                _currentSession = session;
            }

            return OperationResult<DocumentSession>.Success(session.ToDto());
        }
        catch (PdfCoreException error)
        {
            return OperationResult<DocumentSession>.Failure(MapError(error, "create_blank", null, null));
        }
        catch (Exception error)
        {
            return OperationResult<DocumentSession>.Failure(MapUnexpected(error, "create_blank", null, null));
        }
    }

    public Task<OperationResult<DocumentSession>> NavigateAsync(string sessionId, int delta)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
            {
                return Task.FromResult(OperationResult<DocumentSession>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "navigate", sessionId, null)));
            }

            var target = (long)session.PageIndex + delta;
            if (target < 0 || target >= session.Document.PageCount)
            {
                return Task.FromResult(OperationResult<DocumentSession>.Success(session.ToDto()));
            }

            session.PageIndex = (uint)target;
            return Task.FromResult(OperationResult<DocumentSession>.Success(session.ToDto()));
        }
    }

    public Task<RenderResult> RenderCurrentPageAsync(string sessionId, uint dpi, bool invertContentColors)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
            {
                return Task.FromResult(RenderResult.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "render", sessionId, null)));
            }

            return QueueRenderLocked(session, session.PageIndex, dpi, invertContentColors);
        }
    }

    public Task<RenderResult> RenderPageAsync(string sessionId, uint pageIndex, uint dpi, bool invertContentColors)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
            {
                return Task.FromResult(RenderResult.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "render", sessionId, pageIndex)));
            }

            return QueueRenderLocked(session, pageIndex, dpi, invertContentColors);
        }
    }

    /// <summary>Renders one bounded output-space page tile for the continuous viewer.</summary>
    public Task<RenderResult> RenderPageRegionAsync(string sessionId, uint pageIndex, uint dpi, PageRegion region, bool invertContentColors)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
            {
                return Task.FromResult(RenderResult.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "render_region", sessionId, pageIndex)));
            }

            return QueueRenderLocked(session, pageIndex, dpi, invertContentColors, region);
        }
    }

    /// <summary>
    /// Renders every tile covering the viewport on one page, as a single core
    /// call.
    ///
    /// Tile batches run in their own lane, not the page's full-page render
    /// slot: the two describe different things about the same page, and making
    /// them supersede each other would mean a page could never hold both a
    /// coarse base bitmap and sharp tiles over it. A newer batch for the same
    /// page still discards the older one — a new batch means the viewport
    /// moved, so the old one is answering a question nobody is asking.
    /// </summary>
    public Task<TileBatchResult> RenderPageTilesAsync(string sessionId, uint pageIndex, uint dpi, IReadOnlyList<PageRegion> tiles, bool invertContentColors)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
            {
                return Task.FromResult(TileBatchResult.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "render_tiles", sessionId, pageIndex)));
            }

            if (tiles.Count == 0)
            {
                return Task.FromResult(TileBatchResult.Success([]));
            }

            return QueueTileBatchLocked(session, pageIndex, dpi, tiles, invertContentColors);
        }
    }

    /// <summary>
    /// Renders one page at print quality, deliberately independent of the
    /// coalesced viewer render queue so a scrolling page cannot supersede it.
    /// Callers render a document one page at a time and release each page's
    /// pixels before requesting the next, so printing a large document never
    /// holds every page's raw bitmap in memory at once.
    /// </summary>
    public Task<RenderResult> RenderPageForPrintAsync(string sessionId, uint pageIndex, uint dpi, bool invertContentColors)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
            {
                return Task.FromResult(RenderResult.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "print_render", sessionId, pageIndex)));
            }

            if (pageIndex >= session.Document.PageCount)
            {
                return Task.FromResult(RenderResult.Failure(CreateError("The document changed. Please try again.", PdfCoreError.PageIndexOutOfBounds, "print_render", sessionId, pageIndex)));
            }

            session.InFlightPrints++;
            return RenderPageForPrintAsync(session, pageIndex, dpi, invertContentColors);
        }
    }

    public Task<SearchResult> SearchAsync(string sessionId, string query)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
            {
                return Task.FromResult(SearchResult.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "search", sessionId, null)));
            }

            return QueueSearchLocked(session, query);
        }
    }

    public Task<OperationResult<DocumentSession>> NavigateToSearchResultAsync(string sessionId, SearchHit hit)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
            {
                return Task.FromResult(OperationResult<DocumentSession>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "navigate_search", sessionId, hit.PageIndex)));
            }

            if (hit.PageIndex >= session.Document.PageCount)
            {
                return Task.FromResult(OperationResult<DocumentSession>.Failure(CreateError("The document changed. Please try again.", PdfCoreError.PageIndexOutOfBounds, "navigate_search", sessionId, hit.PageIndex)));
            }

            session.PageIndex = hit.PageIndex;
            return Task.FromResult(OperationResult<DocumentSession>.Success(session.ToDto()));
        }
    }

    public void Dispose()
    {
        lock (_gate)
        {
            RetireCurrentSessionLocked();
        }
    }

    /// <summary>
    /// Retires the current session under <see cref="_gate"/>: abandoned render
    /// requests complete as discarded (never left hanging), and the document is
    /// disposed only once no render call is in flight on another thread.
    /// </summary>
    private void RetireCurrentSessionLocked()
    {
        var session = _currentSession;
        _currentSession = null;
        if (session is null)
        {
            return;
        }

        session.Retired = true;
        session.AbandonPendingRenders();
        session.AbandonPendingSearches();
        if (session.HasNoInFlightOperations)
        {
            session.Dispose();
        }
    }

    private Task<SearchResult> QueueSearchLocked(SessionEntry session, string query)
    {
        var request = new SearchRequest(++session.Search.Sequence, query, new TaskCompletionSource<SearchResult>(TaskCreationOptions.RunContinuationsAsynchronously));
        session.Search.Pending?.Completion.TrySetResult(SearchResult.Discarded());
        session.Search.Pending = request;
        if (!session.Search.DispatchScheduled)
        {
            session.Search.DispatchScheduled = true;
            _ = DispatchSearchAsync(session);
        }

        return request.Completion.Task;
    }

    private async Task DispatchSearchAsync(SessionEntry session)
    {
        await Task.Yield();

        SearchRequest request;
        lock (_gate)
        {
            if (_currentSession != session || session.Search.Pending is null)
            {
                session.Search.Pending?.Completion.TrySetResult(SearchResult.Discarded());
                session.Search.Pending = null;
                session.Search.DispatchScheduled = false;
                return;
            }

            request = session.Search.Pending;
            session.Search.Pending = null;
            session.InFlightSearches++;
        }

        SearchResult result;
        try
        {
            var hits = await Task.Run(() => _core.Search(session.Document, request.Query)).ConfigureAwait(false);
            result = SearchResult.Success(new SearchResults(session.Id, request.Sequence, request.Query, [.. hits.Select(hit => new SearchHit(hit.PageIndex, hit.Text, [.. hit.CharacterBounds.Select(bounds => new SearchRect(bounds.XPt, bounds.YPt, bounds.WidthPt, bounds.HeightPt))]))]));
        }
        catch (PdfCoreException error)
        {
            result = SearchResult.Failure(MapError(error, "search", session.Id, null));
        }
        catch (Exception error)
        {
            result = SearchResult.Failure(MapUnexpected(error, "search", session.Id, null));
        }

        lock (_gate)
        {
            session.InFlightSearches--;
            if (session.Retired)
            {
                session.Search.DispatchScheduled = false;
                if (session.HasNoInFlightOperations)
                {
                    session.Dispose();
                }

                request.Completion.TrySetResult(SearchResult.Discarded());
                return;
            }

            var stillCurrent = _currentSession == session && session.Search.Sequence == request.Sequence;
            request.Completion.TrySetResult(stillCurrent ? result : SearchResult.Discarded());
            if (session.Search.Pending is not null)
            {
                _ = DispatchSearchAsync(session);
            }
            else
            {
                session.Search.DispatchScheduled = false;
            }
        }
    }

    private Task<RenderResult> QueueRenderLocked(SessionEntry session, uint pageIndex, uint dpi, bool invertContentColors, PageRegion? region = null)
    {
        var page = session.GetPage(pageIndex);
        var request = new RenderRequest(++page.Sequence, dpi, invertContentColors, region, new TaskCompletionSource<RenderResult>(TaskCreationOptions.RunContinuationsAsynchronously));
        page.Pending?.Completion.TrySetResult(RenderResult.Discarded());
        page.Pending = request;
        if (!page.DispatchScheduled)
        {
            page.DispatchScheduled = true;
            _ = DispatchRenderAsync(session, pageIndex, page);
        }

        return request.Completion.Task;
    }

    private async Task DispatchRenderAsync(SessionEntry session, uint pageIndex, PageRenderState page)
    {
        await Task.Yield();

        RenderRequest request;
        lock (_gate)
        {
            if (_currentSession != session || page.Pending is null)
            {
                page.Pending?.Completion.TrySetResult(RenderResult.Discarded());
                page.Pending = null;
                page.DispatchScheduled = false;
                return;
            }

            request = page.Pending;
            page.Pending = null;
            session.InFlightRenders++;
        }

        RenderResult result;
        try
        {
            var bitmap = await Task.Run(() => request.Region is { } region
                ? _core.RenderPageRegion(session.Document, pageIndex, request.Dpi, region, request.InvertContentColors)
                : _core.RenderPage(session.Document, pageIndex, request.Dpi, request.InvertContentColors)).ConfigureAwait(false);
            result = RenderResult.Success(new RenderedPage(session.Id, pageIndex, request.Sequence, bitmap.Width, bitmap.Height, bitmap.Stride, bitmap.Rgba));
        }
        catch (PdfCoreException error) when (error.Category == PdfCoreError.DocumentNotFound && session.Document.PageCount == 0)
        {
            result = RenderResult.Empty();
        }
        catch (PdfCoreException error)
        {
            result = RenderResult.Failure(MapError(error, "render", session.Id, pageIndex));
        }
        catch (Exception error)
        {
            result = RenderResult.Failure(MapUnexpected(error, "render", session.Id, pageIndex));
        }

        lock (_gate)
        {
            session.InFlightRenders--;
            if (session.Retired)
            {
                // Dispose before completing so awaiting continuations observe
                // the retired document as already released.
                page.DispatchScheduled = false;
                if (session.HasNoInFlightOperations)
                {
                    session.Dispose();
                }

                request.Completion.TrySetResult(RenderResult.Discarded());
                return;
            }

            // Staleness is per page: a newer request for the SAME page (or a
            // session swap) discards this result. Rendering a page that is
            // not the "current" one is deliberate in a continuous viewer.
            var stillCurrent = _currentSession == session && page.Sequence == request.Sequence;
            request.Completion.TrySetResult(stillCurrent ? result : RenderResult.Discarded());

            if (page.Pending is not null)
            {
                _ = DispatchRenderAsync(session, pageIndex, page);
            }
            else
            {
                page.DispatchScheduled = false;
            }
        }
    }

    private Task<TileBatchResult> QueueTileBatchLocked(SessionEntry session, uint pageIndex, uint dpi, IReadOnlyList<PageRegion> tiles, bool invertContentColors)
    {
        var page = session.GetPage(pageIndex);
        var request = new TileBatchRequest(++page.TileSequence, dpi, invertContentColors, tiles, new TaskCompletionSource<TileBatchResult>(TaskCreationOptions.RunContinuationsAsynchronously));
        page.PendingTiles?.Completion.TrySetResult(TileBatchResult.Discarded());
        page.PendingTiles = request;
        if (!page.TileDispatchScheduled)
        {
            page.TileDispatchScheduled = true;
            _ = DispatchTileBatchAsync(session, pageIndex, page);
        }

        return request.Completion.Task;
    }

    private async Task DispatchTileBatchAsync(SessionEntry session, uint pageIndex, PageRenderState page)
    {
        await Task.Yield();

        TileBatchRequest request;
        lock (_gate)
        {
            if (_currentSession != session || page.PendingTiles is null)
            {
                page.PendingTiles?.Completion.TrySetResult(TileBatchResult.Discarded());
                page.PendingTiles = null;
                page.TileDispatchScheduled = false;
                return;
            }

            request = page.PendingTiles;
            page.PendingTiles = null;
            session.InFlightRenders++;
        }

        TileBatchResult result;
        try
        {
            var bitmaps = await Task.Run(() => _core.RenderPageTiles(session.Document, pageIndex, request.Dpi, request.Tiles, request.InvertContentColors)).ConfigureAwait(false);
            result = TileBatchResult.Success([.. bitmaps.Select(bitmap => new RenderedPage(session.Id, pageIndex, request.Sequence, bitmap.Width, bitmap.Height, bitmap.Stride, bitmap.Rgba))]);
        }
        catch (PdfCoreException error)
        {
            result = TileBatchResult.Failure(MapError(error, "render_tiles", session.Id, pageIndex));
        }
        catch (Exception error)
        {
            result = TileBatchResult.Failure(MapUnexpected(error, "render_tiles", session.Id, pageIndex));
        }

        lock (_gate)
        {
            session.InFlightRenders--;
            if (session.Retired)
            {
                page.TileDispatchScheduled = false;
                if (session.HasNoInFlightOperations)
                {
                    session.Dispose();
                }

                request.Completion.TrySetResult(TileBatchResult.Discarded());
                return;
            }

            var stillCurrent = _currentSession == session && page.TileSequence == request.Sequence;
            request.Completion.TrySetResult(stillCurrent ? result : TileBatchResult.Discarded());

            if (page.PendingTiles is not null)
            {
                _ = DispatchTileBatchAsync(session, pageIndex, page);
            }
            else
            {
                page.TileDispatchScheduled = false;
            }
        }
    }

    private async Task<RenderResult> RenderPageForPrintAsync(SessionEntry session, uint pageIndex, uint dpi, bool invertContentColors)
    {
        RenderResult result;
        try
        {
            var bitmap = await Task.Run(() => _core.RenderPage(session.Document, pageIndex, dpi, invertContentColors)).ConfigureAwait(false);
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

            return _currentSession == session ? result : RenderResult.Discarded();
        }
    }

    private bool TryGetCurrentSession(string sessionId, out SessionEntry session)
    {
        session = _currentSession!;
        return session is not null && session.Id == sessionId;
    }

    private UserSafeError MapError(PdfCoreException error, string operation, string? sessionId, uint? pageIndex)
    {
        var message = error.Category switch
        {
            PdfCoreError.PasswordRequired or PdfCoreError.WrongPassword => "This document requires a password.",
            PdfCoreError.UnsupportedSecurityHandler or PdfCoreError.UnsupportedOperation => "This document or action is not supported.",
            PdfCoreError.InvalidImage or PdfCoreError.InvalidSaveRequest => "The requested action could not be completed.",
            PdfCoreError.PageIndexOutOfBounds or PdfCoreError.AnnotationNotFound or PdfCoreError.BitmapNotFound => "The document changed. Please try again.",
            _ => "The document could not be processed."
        };
        return CreateError(message, error.Category, operation, sessionId, pageIndex);
    }

    private UserSafeError MapUnexpected(Exception error, string operation, string? sessionId, uint? pageIndex)
    {
        return CreateError("The document could not be processed.", PdfCoreError.Internal, operation, sessionId, pageIndex, error.GetType().Name);
    }

    private UserSafeError CreateError(string message, PdfCoreError category, string operation, string? sessionId, uint? pageIndex, string sanitizedDetail = "typed_failure")
    {
        var correlationId = Guid.NewGuid().ToString("N");
        _diagnostics.Failure(category, operation, correlationId, sessionId, pageIndex, sanitizedDetail);
        // Both categories mean "the document is encrypted and this password
        // (or its absence) did not unlock it" — the UI treats them the same:
        // prompt and retry. Everything else is a dead-end failure.
        var requiresPassword = category is PdfCoreError.PasswordRequired or PdfCoreError.WrongPassword;
        return new UserSafeError(message, correlationId, requiresPassword);
    }

    private sealed class SessionEntry : IDisposable
    {
        private readonly Dictionary<uint, PageRenderState> _pages = [];

        public SessionEntry(string id, string displayName, IPdfCoreDocument document)
        {
            Id = id;
            DisplayName = displayName;
            Document = document;
        }

        public string Id { get; }
        public string DisplayName { get; }
        public IPdfCoreDocument Document { get; }
        public uint PageIndex { get; set; }
        public int InFlightRenders { get; set; }
        public int InFlightSearches { get; set; }
        public int InFlightPrints { get; set; }
        public bool Retired { get; set; }
        public SearchState Search { get; } = new();
        public bool HasNoInFlightOperations => InFlightRenders == 0 && InFlightSearches == 0 && InFlightPrints == 0;

        public void AbandonPendingRenders()
        {
            foreach (var page in _pages.Values)
            {
                page.Pending?.Completion.TrySetResult(RenderResult.Discarded());
                page.Pending = null;
                page.PendingTiles?.Completion.TrySetResult(TileBatchResult.Discarded());
                page.PendingTiles = null;
            }
        }

        public void AbandonPendingSearches()
        {
            Search.Pending?.Completion.TrySetResult(SearchResult.Discarded());
            Search.Pending = null;
        }

        public PageRenderState GetPage(uint pageIndex)
        {
            if (!_pages.TryGetValue(pageIndex, out var state))
            {
                state = new PageRenderState();
                _pages.Add(pageIndex, state);
            }

            return state;
        }

        public DocumentSession ToDto() => new(
            Id,
            DisplayName,
            Document.PageCount,
            PageIndex,
            Document.PageCount == 0 ? DocumentSessionState.Empty : DocumentSessionState.Ready,
            [.. Document.PageDimensions.Select(page => new PageDimensions(page.WidthPt, page.HeightPt))]);

        public void Dispose() => Document.Dispose();
    }

    private sealed class PageRenderState
    {
        public ulong Sequence { get; set; }
        public bool DispatchScheduled { get; set; }
        public RenderRequest? Pending { get; set; }
        /// <summary>Tile batches are sequenced apart from full-page renders — see <c>RenderPageTilesAsync</c>.</summary>
        public ulong TileSequence { get; set; }
        public bool TileDispatchScheduled { get; set; }
        public TileBatchRequest? PendingTiles { get; set; }
    }

    private sealed record RenderRequest(ulong Sequence, uint Dpi, bool InvertContentColors, PageRegion? Region, TaskCompletionSource<RenderResult> Completion);

    private sealed record TileBatchRequest(ulong Sequence, uint Dpi, bool InvertContentColors, IReadOnlyList<PageRegion> Tiles, TaskCompletionSource<TileBatchResult> Completion);
    private sealed class SearchState
    {
        public ulong Sequence { get; set; }
        public bool DispatchScheduled { get; set; }
        public SearchRequest? Pending { get; set; }
    }

    private sealed record SearchRequest(ulong Sequence, string Query, TaskCompletionSource<SearchResult> Completion);
}
