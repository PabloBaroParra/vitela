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
        if (session.InFlightRenders == 0)
        {
            session.Dispose();
        }
    }

    private Task<RenderResult> QueueRenderLocked(SessionEntry session, uint pageIndex, uint dpi, bool invertContentColors)
    {
        var page = session.GetPage(pageIndex);
        var request = new RenderRequest(++page.Sequence, dpi, invertContentColors, new TaskCompletionSource<RenderResult>(TaskCreationOptions.RunContinuationsAsynchronously));
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
            var bitmap = await Task.Run(() => _core.RenderPage(session.Document, pageIndex, request.Dpi, request.InvertContentColors)).ConfigureAwait(false);
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
                if (session.InFlightRenders == 0)
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
        return new UserSafeError(message, correlationId);
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
        public bool Retired { get; set; }

        public void AbandonPendingRenders()
        {
            foreach (var page in _pages.Values)
            {
                page.Pending?.Completion.TrySetResult(RenderResult.Discarded());
                page.Pending = null;
            }
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
    }

    private sealed record RenderRequest(ulong Sequence, uint Dpi, bool InvertContentColors, TaskCompletionSource<RenderResult> Completion);
}
