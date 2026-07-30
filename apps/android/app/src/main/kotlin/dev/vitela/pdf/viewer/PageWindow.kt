package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PageSize
import kotlin.math.floor
import kotlin.math.sqrt

/**
 * Pages rasterized ahead of and behind the visible range, so a scroll lands
 * on an already-decoded neighbour instead of a spinner.
 */
internal const val PREFETCH_PAGES = 1

/**
 * Pages kept decoded around the visible range. This is a memory budget, not a
 * nicety: at most `visible + 2 * CACHE_PAGES` bitmaps are resident at once, so
 * together with [MAX_PAGE_PIXELS] this bounds peak bitmap heap at roughly
 * 80 MB regardless of document length or screen size. Widening either constant
 * multiplies that — measure before raising it.
 */
internal const val CACHE_PAGES = 2

/**
 * Per-page raster ceiling, ~16 MB as ARGB_8888. This is what stops
 * fit-to-width from scaling peak heap with the size of the display: a page
 * across a phone's 1080 px is ~1.5 Mpx, but the same page unfolded across
 * 2560 px is ~8.5 Mpx, and [CACHE_PAGES] of those at once is an OOM on a
 * device that is not otherwise short of memory.
 */
internal const val MAX_PAGE_PIXELS = 4_000_000

private const val POINTS_PER_INCH = 72.0

/** Density used when the page box is unusable, or before the reader is measured. */
internal const val FALLBACK_RENDER_DPI = 144

internal const val MIN_RENDER_DPI = 1

/**
 * Mirrors the GTK shell's ceiling: a degenerate MediaBox (a page 1 pt wide but
 * thousands of points tall) must not be able to ask for an unbounded raster.
 */
internal const val MAX_RENDER_DPI = 1440

/** US Letter, used until the document's real page box is known. */
internal const val DEFAULT_PAGE_ASPECT_RATIO = 612f / 792f

/**
 * The `[first - radius, last + radius]` band around a visible range, clamped
 * to the document. Empty when the document has no pages.
 */
internal fun pageWindow(first: Int, last: Int, pageCount: Int, radius: Int): IntRange {
    if (pageCount <= 0) return IntRange.EMPTY
    val start = (first - radius).coerceIn(0, pageCount - 1)
    val end = (last + radius).coerceIn(0, pageCount - 1)
    if (start > end) return IntRange.EMPTY
    return start..end
}

/** Drops every cached page that scrolled outside [window]. */
internal fun <T> retainWindow(pages: Map<Int, T>, window: IntRange): Map<Int, T> =
    pages.filterKeys { it in window }

/** Current and transitional bitmaps, each constrained to the cache window. */
internal data class PageBitmaps<T>(val pages: Map<Int, T> = emptyMap(), val bridges: Map<Int, T> = emptyMap())

/** Moves the current generation under the next one without extending the cache window. */
internal fun <T> invalidatePageBitmaps(bitmaps: PageBitmaps<T>, window: IntRange): PageBitmaps<T> =
    PageBitmaps(bridges = retainWindow(bitmaps.bridges + bitmaps.pages, window))

/** Evicts both matching-generation pages and transitional bridges together. */
internal fun <T> retainPageBitmaps(bitmaps: PageBitmaps<T>, window: IntRange): PageBitmaps<T> =
    PageBitmaps(retainWindow(bitmaps.pages, window), retainWindow(bitmaps.bridges, window))

/** A matching-generation bitmap replaces, and releases, its temporary bridge. */
internal fun <T> replacePageBitmap(bitmaps: PageBitmaps<T>, pageIndex: Int, page: T): PageBitmaps<T> =
    PageBitmaps(bitmaps.pages + (pageIndex to page), bitmaps.bridges - pageIndex)

/** Visible pages are rendered before neighbours queued only for prefetch. */
internal fun renderOrder(first: Int, last: Int, pageCount: Int): List<Int> {
    val visible = pageWindow(first, last, pageCount, radius = 0).toList()
    return visible + pageWindow(first, last, pageCount, PREFETCH_PAGES).filter { it !in visible }
}

/** A completion can only update the layout generation that requested it. */
internal fun acceptsRenderCompletion(startedGeneration: Int, currentGeneration: Int, pageIndex: Int, window: IntRange): Boolean =
    startedGeneration == currentGeneration && pageIndex in window

/**
 * Width-to-height ratio for a page's placeholder. Falls back to
 * [DEFAULT_PAGE_ASPECT_RATIO] on a degenerate page box, which would otherwise
 * make `Modifier.aspectRatio` throw.
 */
internal fun PageSize?.aspectRatio(): Float {
    if (this == null || widthPt <= 0.0 || heightPt <= 0.0) return DEFAULT_PAGE_ASPECT_RATIO
    return (widthPt / heightPt).toFloat()
}

/**
 * Fit-to-width density: how densely [size] must rasterize for the page to
 * exactly fill [availableWidthPx] at [zoomFactor], capped by [MAX_PAGE_PIXELS].
 *
 * The cap is the whole reason this is not a one-line division — see
 * [MAX_PAGE_PIXELS]. It trades a little sharpness on very wide viewports for a
 * peak heap that does not depend on the size of the screen.
 */
internal fun renderDpi(size: PageSize?, availableWidthPx: Int, zoomFactor: Double = DEFAULT_ZOOM_FACTOR): Int {
    if (size == null || size.widthPt <= 0.0 || size.heightPt <= 0.0 || availableWidthPx <= 0) {
        return FALLBACK_RENDER_DPI
    }
    val fitToWidth = availableWidthPx / size.widthPt * POINTS_PER_INCH * clampZoomFactor(zoomFactor)
    val budget = sqrt(
        MAX_PAGE_PIXELS * POINTS_PER_INCH * POINTS_PER_INCH / (size.widthPt * size.heightPt),
    )
    // Floor, not round: rounding up steps over the pixel budget.
    return floor(minOf(fitToWidth, budget)).toInt().coerceIn(MIN_RENDER_DPI, MAX_RENDER_DPI)
}

/** A page slot on screen: where its top sits on the scroll axis, and how tall it is. */
internal data class VisiblePage(val index: Int, val offset: Int, val size: Int)

/**
 * The page the reader is actually *on*: the one covering the most of the
 * viewport. Reporting the first visible slot instead pins the counter one page
 * short at the end of a document — the previous page keeps a sliver on screen
 * while the last page fills the rest, so the last page can never be "first".
 *
 * Ties go to the earlier page, which is what a reader scrolling forward
 * expects: the counter advances only once the next page has actually taken over.
 */
internal fun dominantPage(visible: List<VisiblePage>, viewportStart: Int, viewportEnd: Int): Int? =
    visible.maxByOrNull { page ->
        val top = maxOf(page.offset, viewportStart)
        val bottom = minOf(page.offset + page.size, viewportEnd)
        (bottom - top).coerceAtLeast(0)
    }?.index

internal fun visibleRangeStatus(first: Int, last: Int, pageCount: Int): String = when {
    pageCount == 0 -> "No pages."
    first >= last -> "Page ${first + 1} of $pageCount."
    else -> "Pages ${first + 1}-${last + 1} of $pageCount."
}
