package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PageSize
import kotlin.math.abs
import kotlin.math.floor
import kotlin.math.sqrt

/**
 * Width a low-resolution page preview is rasterized to. Enough to read the
 * shape of a page — headings, pictures, columns — while it stands in for the
 * sharp render; at the reader's widest zoom-out ([MIN_ZOOM_FACTOR] across a
 * phone) a page is narrower than this, so there it is not even blurry.
 */
internal const val PREVIEW_WIDTH_PX = 200

/**
 * Per-preview raster ceiling (~240 KB as ARGB_8888), so a degenerate tall
 * page cannot turn a "small" preview into a full-size one.
 */
internal const val PREVIEW_MAX_PIXELS = 60_000

/**
 * Previews kept around the current page, each side. At [MIN_ZOOM_FACTOR] a
 * phone shows ~15 pages at once, so this is two such screens of slack either
 * way — what a fling or a pinch-out reveals before the sharp window catches
 * up. With [PREVIEW_MAX_PIXELS] it bounds previews at `2 * 30 + 1` pages,
 * ~12 MB for Letter pages and under 15 MB whatever their shape: small next to
 * the sharp cache's ~80 MB (see [CACHE_PAGES]).
 */
internal const val PREVIEW_PAGES = 30

private const val POINTS_PER_INCH = 72.0

private val LETTER = PageSize(612.0, 792.0)

/** Density that fits [size] to [PREVIEW_WIDTH_PX], capped by [PREVIEW_MAX_PIXELS]; Letter's on an unusable page box. */
internal fun previewDpi(size: PageSize?): Int {
    val page = size?.takeIf { it.widthPt > 0.0 && it.heightPt > 0.0 } ?: LETTER
    val toWidth = PREVIEW_WIDTH_PX / page.widthPt * POINTS_PER_INCH
    val budget = sqrt(PREVIEW_MAX_PIXELS * POINTS_PER_INCH * POINTS_PER_INCH / (page.widthPt * page.heightPt))
    // Floor, not round: rounding up steps over the width and the pixel budget.
    return floor(minOf(toWidth, budget)).toInt().coerceIn(MIN_RENDER_DPI, MAX_RENDER_DPI)
}

/** The pages a preview is kept for: [PREVIEW_PAGES] either side of [current]. */
internal fun previewWindow(current: Int, pageCount: Int): IntRange = pageWindow(current, current, pageCount, PREVIEW_PAGES)

/**
 * The page to preview next, or null when every page in [window] is [done]:
 * the [visible] pages first, then outward from [current] — the page ahead
 * before the page behind, as a reader moving forward meets them.
 */
internal fun nextPreview(visible: IntRange, current: Int, window: IntRange, done: (Int) -> Boolean): Int? {
    if (window.isEmpty()) return null
    visible.firstOrNull { it in window && !done(it) }?.let { return it }
    for (distance in 0..maxOf(abs(current - window.first), abs(window.last - current))) {
        val ahead = current + distance
        if (ahead in window && !done(ahead)) return ahead
        val behind = current - distance
        if (distance > 0 && behind in window && !done(behind)) return behind
    }
    return null
}

/** A preview lands only for the generation that asked for it, and only on a page still in the window. */
internal fun acceptsPreviewCompletion(startedGeneration: Int, currentGeneration: Int, pageIndex: Int, window: IntRange): Boolean =
    startedGeneration == currentGeneration && pageIndex in window
