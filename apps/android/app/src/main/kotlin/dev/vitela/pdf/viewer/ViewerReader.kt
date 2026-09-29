package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreResult
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The reader: what to rasterize, keep, and evict as the user scrolls and zooms. */
internal class ViewerReader(private val session: ViewerSession) {
    private val state = session.state

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

    /** The last position the list reported, so a layout change can drive the same window again. */
    private var lastPosition: ReaderPosition? = null

    /** A new document replaced the old one: nothing in flight or cached is wanted any more. */
    fun reset() {
        inFlight.clear()
        cacheWindow = IntRange.EMPTY
        lastPosition = null
    }

    /**
     * Makes every render in flight answer a question nobody is asking, without
     * touching what is on screen yet. For the moment before the document swaps
     * the preview those renders read; [layoutChanged] follows once it has.
     */
    fun retireRenders() {
        layoutGeneration += 1
        inFlight.clear()
    }

    /**
     * The pages themselves changed (moved, turned or removed), so every cached
     * bitmap and every render in flight shows a page that is no longer there.
     * Unlike a zoom there is no bridge to keep: an old picture under a new page
     * would be the wrong page. [redrive] asks the window the list last reported
     * to render again; it is off while the grid hides the list, which drives the
     * window itself when it returns.
     */
    fun layoutChanged(redrive: Boolean) {
        layoutGeneration += 1
        inFlight.clear()
        state.value = state.value.copy(pages = emptyMap(), bridgePages = emptyMap())
        val position = lastPosition ?: return
        val last = (state.value.pageCount - 1).coerceAtLeast(0)
        if (redrive && state.value.pageCount > 0) {
            onPositionChanged(
                position.copy(
                    first = position.first.coerceAtMost(last),
                    last = position.last.coerceAtMost(last),
                    current = position.current.coerceAtMost(last),
                ),
            )
        }
    }

    /**
     * The pages stayed where they were but look different — a filled field —
     * so every bitmap is stale. Unlike [layoutChanged] each keeps standing in
     * as a bridge until its redraw lands: the old picture is still the right
     * page, just not its newest value. [redrive] is as for [layoutChanged].
     */
    fun pagesRedrawn(redrive: Boolean) {
        layoutGeneration += 1
        inFlight.clear()
        state.value = state.value.withInvalidatedPageBitmaps(cacheWindow)
        if (redrive) lastPosition?.let(::onPositionChanged)
    }

    /**
     * Drives the first window of a just-opened document rather than waiting
     * for the reader: opening a second document while the list is already
     * parked at page 0 reports an unchanged position, which the reader's
     * distinctUntilChanged would swallow.
     */
    fun showFirstWindow(pageCount: Int) {
        if (pageCount > 0) {
            onPositionChanged(ReaderPosition(0, 0, 0, renderWidthPx, state.value.zoomFactor))
        }
    }

    /**
     * Called by the reader whenever what is on screen changes. This is the
     * single driver of rendering: it decides what to rasterize, what to keep,
     * and what to evict, exactly like the GTK shell's viewport tick.
     */
    fun onPositionChanged(position: ReaderPosition) {
        if (session.document == null) return
        val pageCount = state.value.pageCount
        if (pageCount == 0) return
        // Effects from the old composition may report once while a zoom
        // recomposes. They must not restore its retired render parameters.
        if (position.zoomFactor != state.value.zoomFactor) return
        lastPosition = position
        if (position.viewportWidthPx > 0 && (position.viewportWidthPx != renderWidthPx || position.zoomFactor != renderZoomFactor)) {
            // A rotation, resize, or zoom creates a new bitmap generation. The
            // old cache remains only as a temporary, cache-window-bound bridge.
            renderWidthPx = position.viewportWidthPx
            renderZoomFactor = position.zoomFactor
            layoutGeneration += 1
            inFlight.clear()
            state.value = state.value.withInvalidatedPageBitmaps(cacheWindow)
        }
        cacheWindow = pageWindow(position.first, position.last, pageCount, CACHE_PAGES)
        state.value = state.value.copy(
            pageIndex = boundedPageIndex(position.current, pageCount),
            status = visibleRangeStatus(position.first, position.last, pageCount),
        ).withRetainedPageBitmaps(cacheWindow)
        // Nothing has been measured yet, so there is no width to fit to.
        // The reader's first layout pass calls back with a real one.
        if (renderWidthPx <= 0) return
        for (pageIndex in renderOrder(position.first, position.last, pageCount)) {
            if (pageIndex !in state.value.pages) renderPage(pageIndex)
        }
    }

    /** Consumed by the reader once it has scrolled, so the target fires once. */
    fun consumeScrollTarget() {
        if (state.value.scrollTarget != null) state.value = state.value.copy(scrollTarget = null)
    }

    fun navigate(delta: Int) {
        if (state.value.pageCount == 0) return
        state.value = state.value.copy(scrollTarget = boundedPageIndex(state.value.pageIndex + delta, state.value.pageCount))
    }

    fun zoomIn() = changeZoom(zoomIn(state.value.zoomFactor))

    fun zoomOut() = changeZoom(zoomOut(state.value.zoomFactor))

    private fun changeZoom(zoomFactor: Double) {
        if (session.document == null || zoomFactor == state.value.zoomFactor) return
        // Page geometry changes immediately. Keep cache-window pages beneath the
        // next generation until their sharp replacements arrive.
        layoutGeneration += 1
        inFlight.clear()
        renderWidthPx = 0
        state.value = state.value.copy(zoomFactor = zoomFactor).withInvalidatedPageBitmaps(cacheWindow)
    }

    private fun renderPage(pageIndex: Int) {
        val openDocument = session.document ?: return
        val generation = layoutGeneration
        if (inFlight[pageIndex] == generation) return
        inFlight[pageIndex] = generation
        val dpi = renderDpi(state.value.pageSizes.getOrNull(pageIndex), renderWidthPx, renderZoomFactor)
        session.scope.launch {
            val result = withContext(session.compute) { openDocument.renderPage(pageIndex, dpi) }
            if (inFlight[pageIndex] == generation) inFlight.remove(pageIndex)
            // A render outlives the document that started it when the user
            // opens another file mid-scroll, and outlives its own layout when
            // the device rotates. Either way the bitmap belongs to nobody.
            if (session.document !== openDocument || !acceptsRenderCompletion(generation, layoutGeneration, pageIndex, cacheWindow)) return@launch
            when (result) {
                is PdfCoreResult.Success -> {
                    val bitmap = result.value.toImageBitmap() ?: return@launch
                    state.value = state.value.withReplacementPage(pageIndex, bitmap)
                }
                is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
            }
        }
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
