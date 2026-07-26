package dev.vitela.pdf.viewer

import androidx.compose.ui.graphics.ImageBitmap
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.SearchHit

data class ViewerState(
    val title: String = "No document open",
    val pageCount: Int = 0,
    /** The page filling most of the viewport — what the page counter reports. */
    val pageIndex: Int = 0,
    /** Every page's media box, so unrendered slots still lay out at the right height. */
    val pageSizes: List<PageSize> = emptyList(),
    /**
     * The decoded pages currently resident, keyed by page index. Deliberately
     * sparse: [CACHE_PAGES] bounds how much of a long document may be held at
     * once, and everything outside that window is evicted on scroll.
     */
    val pages: Map<Int, ImageBitmap> = emptyMap(),
    /**
     * A page the reader should scroll to — set by Previous/Next and by search
     * navigation, cleared once the list has consumed it. Null means "the user
     * owns the scroll position", which is the normal case while reading.
     */
    val scrollTarget: Int? = null,
    val searchQuery: String = "",
    val searchHits: List<SearchHit> = emptyList(),
    val searchIndex: Int = 0,
    val status: String = "Select a PDF to begin.",
    val isLoading: Boolean = false,
    val needsPassword: Boolean = false,
    val passwordMessage: String? = null,
    val canPrint: Boolean = false,
    /**
     * Whether a document can be opened at all — false when the native PDF
     * core is not packaged, so the UI can disable its open actions instead of
     * accepting taps it will silently drop. Defaults to false so a state built
     * without deciding this errs on the side of an honest, disabled button.
     */
    val canOpen: Boolean = false,
)

/**
 * What the reader reports back as it scrolls.
 *
 * [first] and [last] bound what is on screen and so drive the render and cache
 * windows; [current] is the page the reader is actually on (see
 * `dominantPage`). They are separate because the edges of the viewport and its
 * centre of gravity are genuinely different questions. [viewportWidthPx] is
 * the width a page slot occupies, which is what fit-to-width rasterizes
 * against — it changes on rotation, and every cached bitmap is stale when it
 * does.
 */
data class ReaderPosition(
    val first: Int,
    val last: Int,
    val current: Int,
    val viewportWidthPx: Int,
)

internal fun boundedPageIndex(pageIndex: Int, pageCount: Int): Int =
    pageIndex.coerceIn(0, (pageCount - 1).coerceAtLeast(0))

internal fun nextSearchIndex(current: Int, count: Int, delta: Int): Int {
    if (count == 0) return 0
    return Math.floorMod(current + delta, count)
}
