package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreResult
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The reader's low-resolution previews ([ViewerState.previewPages]): what a
 * page shows while it has neither a sharp bitmap nor a bridge, so a fling or
 * a pinch-out uncovers pages instead of spinners.
 *
 * They never compete with sharp renders: one preview at a time, started only
 * while [sharpBusy] says no sharp render is running, and [pump]ed again as
 * each sharp render or preview finishes. A preview is zoom-independent, so a
 * zoom keeps them; anything that changes what a page looks like or which page
 * sits where drops them ([invalidate]), and so does a new document ([reset]).
 */
internal class ReaderPreviews(private val session: ViewerSession, private val sharpBusy: () -> Boolean) {
    private val state = session.state

    /** Bumped whenever the previews in flight stop describing the pages; a late one is then dropped. */
    private var generation = 0

    /** The page being previewed for the current [generation], if any: one at a time. */
    private var inFlight: Int? = null

    /** Pages whose preview this generation could not produce, so the pump does not loop on them. */
    private val failed = mutableSetOf<Int>()

    private var window = IntRange.EMPTY
    private var visible = IntRange.EMPTY
    private var current = 0

    /** A new document, or none: nothing kept or in flight belongs to it. */
    fun reset() {
        retire()
        window = IntRange.EMPTY
        visible = IntRange.EMPTY
        current = 0
        state.value = state.value.copy(previewPages = emptyMap())
    }

    /**
     * The preview in flight is reading pages about to be rebuilt; what is on
     * screen stays until [invalidate] or [reset] follows.
     */
    fun retire() {
        generation += 1
        inFlight = null
        failed.clear()
    }

    /**
     * The pages changed — how they look or where they sit — so every preview
     * is stale. Unlike a sharp bitmap none is kept as a bridge: a preview may
     * never outlive an edit to its page. The next [onPositionChanged] renders
     * them again.
     */
    fun invalidate() {
        retire()
        state.value = state.value.copy(previewPages = emptyMap())
    }

    /** The reader moved: keep the window around [current], evict outside it, and fill what is missing. */
    fun onPositionChanged(first: Int, last: Int, current: Int, pageCount: Int) {
        this.current = current
        visible = first..last
        window = previewWindow(current, pageCount)
        failed.retainAll { it in window }
        val kept = retainWindow(state.value.previewPages, window)
        if (kept.size != state.value.previewPages.size) state.value = state.value.copy(previewPages = kept)
        pump()
    }

    /** Starts the next preview, unless one is already running or a sharp render needs the core. */
    fun pump() {
        if (inFlight != null || sharpBusy()) return
        val openDocument = session.document ?: return
        // The grid hides the reader and renders its own thumbnails.
        if (state.value.organize != null) return
        val previews = state.value.previewPages
        val pageIndex = nextPreview(visible, current, window) { it in previews || it in failed } ?: return
        val dpi = previewDpi(state.value.pageSizes.getOrNull(pageIndex))
        val started = generation
        inFlight = pageIndex
        session.scope.launch {
            val bitmap = withContext(session.compute) { (openDocument.renderPage(pageIndex, dpi) as? PdfCoreResult.Success)?.value?.toImageBitmap() }
            // A newer generation owns the slot (and pumps for itself), and a
            // preview of the previous document belongs to nobody.
            if (started != generation || session.document !== openDocument) return@launch
            inFlight = null
            when {
                !acceptsPreviewCompletion(started, generation, pageIndex, window) -> Unit
                bitmap == null -> failed += pageIndex
                else -> state.value = state.value.copy(previewPages = state.value.previewPages + (pageIndex to bitmap))
            }
            pump()
        }
    }
}
