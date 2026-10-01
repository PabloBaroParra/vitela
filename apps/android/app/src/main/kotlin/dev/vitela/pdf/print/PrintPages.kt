package dev.vitela.pdf.print

/**
 * The pure half of printing — which pages a job asks for and where a raster
 * lands on the sheet. No `android.*` here, so the JVM tests cover it; the
 * adapter only maps the framework's `PageRange`/`RectF` onto these types.
 */

/**
 * The zero-based pages of a [pageCount]-page document that [ranges] ask for,
 * ascending and without repeats. Ranges are clamped to the document, so
 * `PageRange.ALL_PAGES` (`0..Int.MAX_VALUE`) is simply every page.
 */
internal fun pagesToPrint(ranges: List<IntRange>, pageCount: Int): List<Int> {
    if (pageCount <= 0) return emptyList()
    val pages = sortedSetOf<Int>()
    for (range in ranges) {
        val first = maxOf(range.first, 0)
        val last = minOf(range.last, pageCount - 1)
        for (page in first..last) pages += page
    }
    return pages.toList()
}

/** [pages] (ascending) folded into the fewest inclusive ranges: what `onWriteFinished` reports. */
internal fun writtenRanges(pages: List<Int>): List<IntRange> {
    val ranges = mutableListOf<IntRange>()
    for (page in pages) {
        val previous = ranges.lastOrNull()
        if (previous != null && previous.last + 1 == page) ranges[ranges.lastIndex] = previous.first..page
        else ranges += page..page
    }
    return ranges
}

/** A raster drawn at [scale], its top-left corner [dx] and [dy] from the box's. */
internal data class Placement(val scale: Float, val dx: Float, val dy: Float)

/**
 * Fits a [srcWidth] by [srcHeight] raster into a [boxWidth] by [boxHeight]
 * box by the smaller axis ratio — aspect preserved, never cropped — and
 * centres it on the other axis. Null when any side is not a positive number.
 */
internal fun fitCentered(srcWidth: Float, srcHeight: Float, boxWidth: Float, boxHeight: Float): Placement? {
    val sides = floatArrayOf(srcWidth, srcHeight, boxWidth, boxHeight)
    if (sides.any { !it.isFinite() || it <= 0f }) return null
    val scale = minOf(boxWidth / srcWidth, boxHeight / srcHeight)
    return Placement(scale, (boxWidth - srcWidth * scale) / 2f, (boxHeight - srcHeight * scale) / 2f)
}
