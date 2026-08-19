namespace Pdf.Windows.Facade;

public sealed class PdfDocumentFacade : IDisposable
{
    private readonly IPdfCore _core;
    private readonly IDiagnosticLogger _diagnostics;
    private readonly object _gate = new();
    private readonly SemaphoreSlim _documentChangeGate = new(1, 1);
    private SessionEntry? _currentSession;

    internal PdfDocumentFacade(IPdfCore core, IDiagnosticLogger diagnostics)
    {
        _core = core;
        _diagnostics = diagnostics;
    }

    /// <summary>
    /// Opens a document, refusing while the current one holds unsaved
    /// annotation work. <paramref name="discardPendingEdits"/> is how the shell
    /// says the reader was asked and chose to lose it: the guard exists to make
    /// that loss deliberate, not to make it impossible.
    /// </summary>
    public async Task<OperationResult<DocumentSession>> OpenAsync(DocumentSource source, string? password = null, bool discardPendingEdits = false)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            lock (_gate)
            {
                if (!discardPendingEdits && _currentSession is { } current && current.HasUnsavedEdits(_core))
                {
                    return OperationResult<DocumentSession>.Failure(CreateError("Save or undo the pending annotation changes before opening another document.", PdfCoreError.UnsavedChanges, "open", current.Id, null));
                }
            }

            var document = await Task.Run(() => _core.OpenFromBytes(source.Bytes, password)).ConfigureAwait(false);
            var session = new SessionEntry(Guid.NewGuid().ToString("N"), source.DisplayName, document, _core.ContentEditingAllowed(document));
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
        finally
        {
            _documentChangeGate.Release();
        }
    }

    public OperationResult<DocumentSession> OpenReadFailure(Exception error)
    {
        return OperationResult<DocumentSession>.Failure(MapUnexpected(error, "open", null, null));
    }

    public OperationResult<SavedDocument> SaveWriteFailure(Exception error)
    {
        return OperationResult<SavedDocument>.Failure(MapUnexpected(error, "save", null, null));
    }

    public async Task<OperationResult<DocumentSession>> CreateBlankAsync(bool discardPendingEdits = false)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            lock (_gate)
            {
                if (!discardPendingEdits && _currentSession is { } current && current.HasUnsavedEdits(_core))
                {
                    return OperationResult<DocumentSession>.Failure(CreateError("Save or undo the pending annotation changes before creating another document.", PdfCoreError.UnsavedChanges, "create_blank", current.Id, null));
                }
            }

            var document = await Task.Run(_core.CreateBlank).ConfigureAwait(false);
            var session = new SessionEntry(Guid.NewGuid().ToString("N"), "Untitled", document, _core.ContentEditingAllowed(document));
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
        finally
        {
            _documentChangeGate.Release();
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

    /// <summary>
    /// Loads and flattens one page's characters for caret hit-testing and
    /// selection-rect queries. Dispatched off the UI thread because it reads
    /// text runs from pdfium; the returned handle is then queried
    /// synchronously by the shell on every pointer-move of a drag-select — no
    /// per-move round trip.
    /// </summary>
    public async Task<OperationResult<PageCharacters>> PageCharactersAsync(string sessionId, uint pageIndex)
    {
        SessionEntry session;
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out session))
            {
                return OperationResult<PageCharacters>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "page_characters", sessionId, pageIndex));
            }
        }

        try
        {
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
        catch (PdfCoreException error)
        {
            return OperationResult<PageCharacters>.Failure(MapError(error, "page_characters", sessionId, pageIndex));
        }
        catch (Exception error)
        {
            return OperationResult<PageCharacters>.Failure(MapUnexpected(error, "page_characters", sessionId, pageIndex));
        }
    }

    public Task<OperationResult<AnnotationState>> AnnotationStateAsync(string sessionId)
    {
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out var session))
            {
                return Task.FromResult(OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "annotation_state", sessionId, null)));
            }

            return Task.FromResult(OperationResult<AnnotationState>.Success(session.AnnotationState(_core)));
        }
    }

    internal async Task<OperationResult<AnnotationState>> EditAnnotationAsync(string sessionId, PdfCoreEdit edit)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out var session))
                {
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "annotation_edit", sessionId, null));
                }
                try
                {
                    _core.ApplyEdit(session.Document, edit);
                    session.EditRevision++;
                    return OperationResult<AnnotationState>.Success(session.AnnotationState(_core));
                }
                catch (PdfCoreException error)
                {
                    return OperationResult<AnnotationState>.Failure(MapError(error, "annotation_edit", sessionId, null));
                }
            }
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    /// <summary>
    /// Where a dropped or pasted image should land, anchored by its top-left
    /// corner at (<paramref name="anchorX"/>, <paramref name="anchorY"/>) in
    /// PDF space.
    ///
    /// Takes no session and no lock: <c>stamp_placement</c> reads the image
    /// bytes and nothing else, so it cannot observe or disturb a document. It
    /// exists so the shell never sizes a stamp itself — the image's own
    /// proportions decide, in the core, for every shell alike.
    /// </summary>
    internal OperationResult<PdfCoreRect> StampPlacement(byte[] imageBytes, double anchorX, double anchorY)
    {
        try
        {
            return OperationResult<PdfCoreRect>.Success(_core.StampPlacement(imageBytes, anchorX, anchorY));
        }
        catch (PdfCoreException error)
        {
            return OperationResult<PdfCoreRect>.Failure(MapError(error, "annotation_edit", null, null));
        }
    }

    /// <summary>
    /// Inserts a Stamp annotation — kept separate from <see cref="EditAnnotationAsync"/>
    /// because <c>insert_image_stamp</c> is its own FFI entrypoint (image bytes,
    /// not a <c>PdfCoreEdit</c> value), but otherwise follows the same
    /// gate-lock-apply-bump-revision shape.
    /// </summary>
    internal async Task<OperationResult<AnnotationState>> InsertStampAsync(string sessionId, uint pageIndex, byte[] imageBytes, PdfCoreRect rect)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out var session))
                {
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "annotation_edit", sessionId, null));
                }
                try
                {
                    _core.InsertImageStamp(session.Document, pageIndex, imageBytes, rect);
                    session.EditRevision++;
                    return OperationResult<AnnotationState>.Success(session.AnnotationState(_core));
                }
                catch (PdfCoreException error)
                {
                    return OperationResult<AnnotationState>.Failure(MapError(error, "annotation_edit", sessionId, pageIndex));
                }
            }
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }


    /// <summary>
    /// Parses one page's editable content — the text runs its content stream
    /// paints. Dispatched off the UI thread: this re-parses the page's stream
    /// every call, since the core deliberately never caches page content the
    /// way it caches annotations.
    /// </summary>
    /// <remarks>
    /// Refused outright on a document whose permissions withhold content
    /// editing, rather than read and then refused at the edit: offering a
    /// reader runs they are not allowed to change is worse than telling them
    /// once.
    /// </remarks>
    public async Task<OperationResult<PageContent>> PageContentAsync(string sessionId, uint pageIndex)
    {
        SessionEntry session;
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out session))
            {
                return OperationResult<PageContent>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "page_content", sessionId, pageIndex));
            }

            if (!session.ContentEditingAllowed)
            {
                return OperationResult<PageContent>.Failure(CreateError("This document does not permit content changes.", PdfCoreError.UnsupportedOperation, "page_content", sessionId, pageIndex));
            }
        }

        try
        {
            var content = await Task.Run(() => _core.ReadPageContent(session.Document, pageIndex)).ConfigureAwait(false);
            lock (_gate)
            {
                if (session.Retired || _currentSession != session)
                {
                    // The ids in this parse belong to bytes nobody is looking
                    // at any more; handing them back would let a later edit
                    // target the wrong document.
                    return OperationResult<PageContent>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "page_content", sessionId, pageIndex));
                }
            }

            return OperationResult<PageContent>.Success(new PageContent(pageIndex, [.. content.TextRuns.Select(run => new ContentTextRun(run))]));
        }
        catch (PdfCoreException error)
        {
            return OperationResult<PageContent>.Failure(MapError(error, "page_content", sessionId, pageIndex));
        }
        catch (Exception error)
        {
            return OperationResult<PageContent>.Failure(MapUnexpected(error, "page_content", sessionId, pageIndex));
        }
    }

    /// <summary>
    /// Retypes <paramref name="run"/> as <paramref name="text"/>, keeping the
    /// run's font, size and position, then rebuilds the preview so the page
    /// shows it.
    /// </summary>
    /// <remarks>
    /// The refresh is not optional and not the caller's to skip: retyped text
    /// is drawn by the PDF itself, so until the render side is re-derived the
    /// reader is looking at the old words with a status line claiming
    /// otherwise. It is also why this returns only after the refresh — the
    /// shell re-renders on the result.
    ///
    /// Retyping the same run twice folds into the first edit rather than
    /// queueing a second, which the core handles; the caller may simply send
    /// the run it read and the text it wants.
    /// </remarks>
    public async Task<OperationResult<AnnotationState>> ReplaceTextRunAsync(string sessionId, ContentTextRun run, string text)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        SessionEntry session;
        try
        {
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                {
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "content_edit", sessionId, run.PageIndex));
                }

                try
                {
                    _core.ApplyEdit(session.Document, new PdfCoreEdit.ReplaceTextRun(run.Source, text));
                    session.EditRevision++;
                    session.HasRecordedContentEdit = true;
                }
                catch (PdfCoreException error)
                {
                    return OperationResult<AnnotationState>.Failure(MapError(error, "content_edit", sessionId, run.PageIndex));
                }
            }

            return await RefreshPreviewAsync(session, "content_edit", run.PageIndex).ConfigureAwait(false);
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    /// <summary>
    /// Rebuilds what rendering draws from the edits queued so far, off the UI
    /// thread — it saves a snapshot in memory and reopens it, which is real
    /// work on a large document.
    /// </summary>
    /// <remarks>
    /// A failure here is reported but never rolls the edit back. The command
    /// stays recorded and undoable, and if the failure is real rather than a
    /// stale session the eventual save hits the same error with the same
    /// explanation. Dropping an edit the reader already confirmed, to keep a
    /// preview honest, is the worse trade.
    /// </remarks>
    private async Task<OperationResult<AnnotationState>> RefreshPreviewAsync(SessionEntry session, string operation, uint? pageIndex)
    {
        try
        {
            await Task.Run(() => _core.RefreshPreview(session.Document)).ConfigureAwait(false);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<AnnotationState>.Failure(MapError(error, operation, session.Id, pageIndex));
        }
        catch (Exception error)
        {
            return OperationResult<AnnotationState>.Failure(MapUnexpected(error, operation, session.Id, pageIndex));
        }

        lock (_gate)
        {
            if (session.Retired || _currentSession != session)
            {
                return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, operation, session.Id, pageIndex));
            }

            return OperationResult<AnnotationState>.Success(session.AnnotationState(_core));
        }
    }

    public Task<OperationResult<AnnotationState>> UndoAsync(string sessionId) => HistoryAsync(sessionId, undo: true);
    public Task<OperationResult<AnnotationState>> RedoAsync(string sessionId) => HistoryAsync(sessionId, undo: false);

    /// <summary>
    /// Whether saving this session would break a signature the document
    /// already carries — the question the shell asks so it can warn before
    /// the reader commits to a save.
    /// </summary>
    /// <remarks>
    /// Asking this and then saving is not a race worth guarding: more edits
    /// arriving in between can only move the answer from false to true, and a
    /// save that became signature-breaking without an acknowledgement is
    /// refused by the core anyway. The reader gets told either way; the only
    /// difference is whether they are told as a question or as a failure.
    /// </remarks>
    public async Task<OperationResult<bool>> WillInvalidateSignaturesAsync(string sessionId)
    {
        SessionEntry session;
        lock (_gate)
        {
            if (!TryGetCurrentSession(sessionId, out session))
            {
                return OperationResult<bool>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "signatures", sessionId, null));
            }
        }

        try
        {
            // Off the UI thread: answering means scanning every object for a
            // signature dictionary, which on a large document is long enough
            // to be felt as a stutter.
            var invalidates = await Task.Run(() => _core.WillInvalidateSignatures(session.Document)).ConfigureAwait(false);
            return OperationResult<bool>.Success(invalidates);
        }
        catch (PdfCoreException error)
        {
            return OperationResult<bool>.Failure(MapError(error, "signatures", sessionId, null));
        }
        catch (Exception error)
        {
            return OperationResult<bool>.Failure(MapUnexpected(error, "signatures", sessionId, null));
        }
    }

    /// <param name="signaturesAcknowledged">
    /// Pass <c>true</c> only after the reader has been told the save breaks an
    /// existing signature and chose to continue — see
    /// <see cref="WillInvalidateSignaturesAsync"/>. The default is <c>false</c>
    /// on purpose: a caller that has not thought about signatures gets a
    /// refusal, not a broken one.
    /// </param>
    public async Task<OperationResult<SavedDocument>> SaveToDestinationAsync(string sessionId, Func<byte[], Task> replaceDestination, bool signaturesAcknowledged = false)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        SessionEntry session;
        ulong revision;
        try
        {
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                {
                    return OperationResult<SavedDocument>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "save", sessionId, null));
                }
                revision = session.EditRevision;
            }

            var bytes = await Task.Run(() => _core.SaveToBytes(session.Document, signaturesAcknowledged)).ConfigureAwait(false);
            await replaceDestination(bytes).ConfigureAwait(false);
            lock (_gate)
            {
                session.SavedRevision = revision;
                return OperationResult<SavedDocument>.Success(new SavedDocument(bytes, revision));
            }
        }
        catch (PdfCoreException error)
        {
            return OperationResult<SavedDocument>.Failure(MapError(error, "save", sessionId, null));
        }
        catch (Exception error)
        {
            return OperationResult<SavedDocument>.Failure(MapUnexpected(error, "save", sessionId, null));
        }
        finally
        {
            _documentChangeGate.Release();
        }
    }

    private async Task<OperationResult<AnnotationState>> HistoryAsync(string sessionId, bool undo)
    {
        var operation = undo ? "undo" : "redo";
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        SessionEntry session;
        try
        {
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                {
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, operation, sessionId, null));
                }
                try
                {
                    if (undo ? _core.Undo(session.Document) : _core.Redo(session.Document))
                    {
                        session.EditRevision++;
                    }
                    else
                    {
                        return OperationResult<AnnotationState>.Success(session.AnnotationState(_core));
                    }
                }
                catch (PdfCoreException error)
                {
                    return OperationResult<AnnotationState>.Failure(MapError(error, operation, sessionId, null));
                }

                if (!session.HasRecordedContentEdit)
                {
                    // Annotation-only history: the shell redraws its own
                    // overlays and the bitmap underneath never changed.
                    return OperationResult<AnnotationState>.Success(session.AnnotationState(_core));
                }
            }

            // A session that has touched page content refreshes on every step,
            // without asking which command moved: the step that *removes* the
            // last content edit is exactly the one whose preview would
            // otherwise keep showing it.
            return await RefreshPreviewAsync(session, operation, null).ConfigureAwait(false);
        }
        finally
        {
            _documentChangeGate.Release();
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
            PdfCoreError.InvalidImage => "The image is corrupt or unsupported.",
            // Named, and with the character in it: this is the one failure the
            // reader can clear themselves, by typing something else.
            PdfCoreError.EncodingGap => error.ReaderFacingDetail is { Length: > 0 } character
                ? $"This text's font cannot show \"{character}\". Try different characters."
                : "This text's font cannot show one of those characters.",
            PdfCoreError.InvalidSaveRequest => "The requested action could not be completed.",
            // Named rather than folded into the generic message: the user can
            // act on it (save a copy elsewhere, or keep the signed original),
            // and "could not be processed" would read like a bug in Vitela.
            PdfCoreError.SignaturesWouldBeInvalidated => "Saving would break this document's digital signature, so it was not saved.",
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
        return new UserSafeError(message, correlationId, requiresPassword, category is PdfCoreError.UnsavedChanges);
    }

    private sealed class SessionEntry : IDisposable
    {
        private readonly Dictionary<uint, PageRenderState> _pages = [];

        public SessionEntry(string id, string displayName, IPdfCoreDocument document, bool contentEditingAllowed)
        {
            Id = id;
            DisplayName = displayName;
            Document = document;
            ContentEditingAllowed = contentEditingAllowed;
        }

        public string Id { get; }
        public string DisplayName { get; }
        public IPdfCoreDocument Document { get; }
        public uint PageIndex { get; set; }
        public int InFlightRenders { get; set; }
        public int InFlightSearches { get; set; }
        public int InFlightPrints { get; set; }
        public bool Retired { get; set; }
        public ulong EditRevision { get; set; }
        public ulong SavedRevision { get; set; }

        /// <summary>Whether the document's permissions allow rewriting page content.</summary>
        public bool ContentEditingAllowed { get; }

        /// <summary>
        /// Whether this session has ever recorded a page-content edit, and so
        /// owes the render side a refresh after an undo or redo.
        ///
        /// Latched on rather than recounted: undoing the last content edit is
        /// exactly the moment the preview must be rebuilt to drop it, so an
        /// "are any pending right now" answer would skip the one refresh that
        /// matters most. Annotation-only sessions never set it and never pay
        /// for a refresh they would only have to undo.
        /// </summary>
        public bool HasRecordedContentEdit { get; set; }

        /// <summary>
        /// Whether replacing this document would throw away annotation work.
        ///
        /// Both halves are needed, and neither is enough alone. The revision
        /// counter only ever climbs — undo bumps it too, being an operation
        /// like any other — so on its own it could never return to clean and
        /// the reader had no way out but to save. The edit log's undo stack
        /// answers that half: emptied, there is nothing left to lose. But the
        /// stack alone would keep reporting work after a save, when the file
        /// on disk already holds every edit still sitting in it.
        ///
        /// Mirrors the Linux shell's <c>has_pending_annotation_edits</c>,
        /// which asks the same <c>pending_edits.can_undo()</c> question.
        /// </summary>
        public bool HasUnsavedEdits(IPdfCore core) => EditRevision != SavedRevision && core.CanUndo(Document);
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
            [.. Document.PageDimensions.Select(page => new PageDimensions(page.WidthPt, page.HeightPt))],
            ContentEditingAllowed);

        public AnnotationState AnnotationState(IPdfCore core) => new(
            Id,
            [.. core.Annotations(Document).Select(annotation => new Annotation(
                annotation.Id,
                annotation.PageIndex,
                (AnnotationKind)annotation.Kind,
                annotation.Rect is null ? null : new AnnotationRect(annotation.Rect.X, annotation.Rect.Y, annotation.Rect.Width, annotation.Rect.Height),
                annotation.Color is null ? null : new AnnotationColor(annotation.Color.R, annotation.Color.G, annotation.Color.B),
                [.. annotation.Points.Select(point => new AnnotationPoint(point.X, point.Y))]))],
            core.AnnotationEditingAllowed(Document),
            core.CanUndo(Document),
            core.CanRedo(Document));

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

    private sealed record RenderRequest(ulong Sequence, uint Dpi, bool InvertContentColors, TaskCompletionSource<RenderResult> Completion);

    private sealed record TileBatchRequest(ulong Sequence, uint Dpi, bool InvertContentColors, IReadOnlyList<PageRegion> Tiles, TaskCompletionSource<TileBatchResult> Completion);
    private sealed class SearchState
    {
        public ulong Sequence { get; set; }
        public bool DispatchScheduled { get; set; }
        public SearchRequest? Pending { get; set; }
    }

    private sealed record SearchRequest(ulong Sequence, string Query, TaskCompletionSource<SearchResult> Completion);
}
