package dev.vitela.pdf.viewer

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class ViewerViewModel(private val core: PdfCore?) : ViewModel() {
    private val _state = MutableStateFlow(ViewerState(status = availabilityMessage(core), canOpen = core != null))
    val state: StateFlow<ViewerState> = _state.asStateFlow()
    private var sourceBytes: ByteArray? = null
    private var document: PdfDocument? = null

    /**
     * Pages with a render in flight, mapped to the [layoutGeneration] they
     * were started for, so a scroll tick never queues the same page twice and
     * a render left over from the previous layout cannot evict the entry
     * belonging to its replacement.
     */
    private val inFlight = mutableMapOf<Int, Int>()

    /**
     * The window a finished render is still wanted for. A render that lands
     * after the user scrolled past its page must be dropped, not cached —
     * otherwise fast scrolling grows the resident set past [CACHE_PAGES].
     */
    private var cacheWindow: IntRange = IntRange.EMPTY

    /** Slot width every cached bitmap was rasterized for. Zero until the reader is laid out. */
    private var renderWidthPx = 0
    private var renderZoomFactor = DEFAULT_ZOOM_FACTOR

    /**
     * Bumped whenever [renderWidthPx] changes. Fit-to-width ties a bitmap to
     * the width it was rendered for, so a rotation invalidates the whole cache
     * *and* every render already in flight; the generation is how a finished
     * render knows it is answering a question nobody is asking any more.
     */
    private var layoutGeneration = 0

    fun open(displayName: String, bytes: ByteArray, password: String? = null) {
        val availableCore = core ?: return
        // Retain only the selected bytes while the session is active. SAF URIs
        // are not paths and passwords are never retained after this call.
        sourceBytes = bytes
        _state.value = _state.value.copy(title = displayName, isLoading = true, needsPassword = false, passwordMessage = null, status = "Opening PDF...")
        viewModelScope.launch {
            when (val result = withContext(Dispatchers.Default) { availableCore.openFromBytes(bytes, password) }) {
                is PdfCoreResult.Success -> {
                    document?.close()
                    document = result.value
                    inFlight.clear()
                    cacheWindow = IntRange.EMPTY
                    val pageCount = result.value.pageCount
                    // A fresh state, not a copy: the previous document's pages,
                    // search hits and password flags must not survive. canOpen
                    // is true by construction — reaching here proves the core
                    // is present.
                    _state.value = ViewerState(
                        title = displayName,
                        pageCount = pageCount,
                        pageSizes = withContext(Dispatchers.Default) { result.value.pageSizes },
                        // Scroll back to the top: the list may still be parked
                        // deep inside the document that was just replaced.
                        scrollTarget = if (pageCount > 0) 0 else null,
                        status = visibleRangeStatus(0, 0, pageCount),
                        canPrint = pageCount > 0,
                        canOpen = true,
                    )
                    // Drive the first window from here rather than waiting for
                    // the reader: opening a second document while the list is
                    // already parked at page 0 reports an unchanged position,
                    // which the reader's distinctUntilChanged would swallow.
                    if (pageCount > 0) {
                        onReaderPositionChanged(ReaderPosition(0, 0, 0, renderWidthPx, _state.value.zoomFactor))
                    }
                }
                is PdfCoreResult.Failure -> handleOpenFailure(result.error)
            }
        }
    }

    fun retryPassword(password: String) {
        val bytes = sourceBytes ?: return
        open(_state.value.title, bytes, password)
    }

    fun reportReadFailure() {
        _state.value = _state.value.copy(status = "Could not read the selected PDF.")
    }

    /**
     * Called by the reader whenever what is on screen changes. This is the
     * single driver of rendering: it decides what to rasterize, what to keep,
     * and what to evict, exactly like the GTK shell's viewport tick.
     */
    fun onReaderPositionChanged(position: ReaderPosition) {
        if (document == null) return
        val pageCount = _state.value.pageCount
        if (pageCount == 0) return
        // Effects from the old composition may report once while a zoom
        // recomposes. They must not restore its retired render parameters.
        if (position.zoomFactor != _state.value.zoomFactor) return
        if (position.viewportWidthPx > 0 && (position.viewportWidthPx != renderWidthPx || position.zoomFactor != renderZoomFactor)) {
            // A rotation, resize, or zoom creates a new bitmap generation. The
            // old cache remains only as a temporary, cache-window-bound bridge.
            renderWidthPx = position.viewportWidthPx
            renderZoomFactor = position.zoomFactor
            layoutGeneration += 1
            inFlight.clear()
            _state.value = _state.value.withInvalidatedPageBitmaps(cacheWindow)
        }
        cacheWindow = pageWindow(position.first, position.last, pageCount, CACHE_PAGES)
        _state.value = _state.value.copy(
            pageIndex = boundedPageIndex(position.current, pageCount),
            status = visibleRangeStatus(position.first, position.last, pageCount),
        ).withRetainedPageBitmaps(cacheWindow)
        // Nothing has been measured yet, so there is no width to fit to.
        // The reader's first layout pass calls back with a real one.
        if (renderWidthPx <= 0) return
        for (pageIndex in renderOrder(position.first, position.last, pageCount)) {
            if (pageIndex !in _state.value.pages) renderPage(pageIndex)
        }
    }

    /** Consumed by the reader once it has scrolled, so the target fires once. */
    fun consumeScrollTarget() {
        if (_state.value.scrollTarget != null) _state.value = _state.value.copy(scrollTarget = null)
    }

    fun navigate(delta: Int) {
        if (_state.value.pageCount == 0) return
        _state.value = _state.value.copy(scrollTarget = boundedPageIndex(_state.value.pageIndex + delta, _state.value.pageCount))
    }

    fun zoomIn() = changeZoom(zoomIn(_state.value.zoomFactor))

    fun zoomOut() = changeZoom(zoomOut(_state.value.zoomFactor))

    fun search(query: String) {
        val openDocument = document ?: return
        viewModelScope.launch {
            _state.value = _state.value.copy(searchQuery = query, status = "Searching...")
            when (val result = withContext(Dispatchers.Default) { openDocument.search(query) }) {
                is PdfCoreResult.Success -> {
                    val hit = result.value.firstOrNull()
                    _state.value = _state.value.copy(
                        searchHits = result.value,
                        searchIndex = 0,
                        scrollTarget = hit?.pageIndex,
                        status = if (result.value.isEmpty()) "No matches." else "Match 1 of ${result.value.size}.",
                    )
                }
                is PdfCoreResult.Failure -> _state.value = _state.value.copy(status = userMessage(result.error))
            }
        }
    }

    fun stepSearch(delta: Int) {
        val hits = _state.value.searchHits
        val index = nextSearchIndex(_state.value.searchIndex, hits.size, delta)
        val hit = hits.getOrNull(index) ?: return
        _state.value = _state.value.copy(searchIndex = index, scrollTarget = hit.pageIndex, status = "Match ${index + 1} of ${hits.size}.")
    }

    fun printBytes(): ByteArray? = sourceBytes

    private fun renderPage(pageIndex: Int) {
        val openDocument = document ?: return
        val generation = layoutGeneration
        if (inFlight[pageIndex] == generation) return
        inFlight[pageIndex] = generation
        val dpi = renderDpi(_state.value.pageSizes.getOrNull(pageIndex), renderWidthPx, renderZoomFactor)
        viewModelScope.launch {
            val result = withContext(Dispatchers.Default) { openDocument.renderPage(pageIndex, dpi) }
            if (inFlight[pageIndex] == generation) inFlight.remove(pageIndex)
            // A render outlives the document that started it when the user
            // opens another file mid-scroll, and outlives its own layout when
            // the device rotates. Either way the bitmap belongs to nobody.
            if (document !== openDocument || !acceptsRenderCompletion(generation, layoutGeneration, pageIndex, cacheWindow)) return@launch
            when (result) {
                is PdfCoreResult.Success -> {
                    val bitmap = result.value.toImageBitmap() ?: return@launch
                    _state.value = _state.value.withReplacementPage(pageIndex, bitmap)
                }
                is PdfCoreResult.Failure -> _state.value = _state.value.copy(status = userMessage(result.error))
            }
        }
    }

    private fun handleOpenFailure(error: PdfCoreError) {
        _state.value = _state.value.copy(isLoading = false, needsPassword = error is PdfCoreError.PasswordRequired || error is PdfCoreError.WrongPassword, passwordMessage = if (error is PdfCoreError.WrongPassword) "The password is incorrect. Try again." else null, status = userMessage(error))
    }

    private fun changeZoom(zoomFactor: Double) {
        if (document == null || zoomFactor == _state.value.zoomFactor) return
        // Page geometry changes immediately. Keep cache-window pages beneath the
        // next generation until their sharp replacements arrive.
        layoutGeneration += 1
        inFlight.clear()
        renderWidthPx = 0
        _state.value = _state.value.copy(zoomFactor = zoomFactor).withInvalidatedPageBitmaps(cacheWindow)
    }
}

private fun ViewerState.withInvalidatedPageBitmaps(window: IntRange): ViewerState {
    val bitmaps = invalidatePageBitmaps(PageBitmaps(pages, bridgePages), window)
    return copy(pages = bitmaps.pages, bridgePages = bitmaps.bridges)
}

private fun ViewerState.withRetainedPageBitmaps(window: IntRange): ViewerState {
    val bitmaps = retainPageBitmaps(PageBitmaps(pages, bridgePages), window)
    return copy(pages = bitmaps.pages, bridgePages = bitmaps.bridges)
}

private fun ViewerState.withReplacementPage(pageIndex: Int, page: androidx.compose.ui.graphics.ImageBitmap): ViewerState {
    val bitmaps = replacePageBitmap(PageBitmaps(pages, bridgePages), pageIndex, page)
    return copy(pages = bitmaps.pages, bridgePages = bitmaps.bridges)
}

private fun availabilityMessage(core: PdfCore?): String = if (core == null) "Native PDF support is not packaged. Build with scripts/package-android.sh and externally supplied PDFium libraries." else "Select a PDF to begin."

private fun userMessage(error: PdfCoreError): String = when (error) {
    PdfCoreError.PasswordRequired, PdfCoreError.WrongPassword -> "This document requires a password."
    is PdfCoreError.Failed -> error.message
}
