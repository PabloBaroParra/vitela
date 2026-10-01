package dev.vitela.pdf.viewer

import androidx.compose.ui.graphics.ImageBitmap
import dev.vitela.pdf.core.DocumentBlock
import dev.vitela.pdf.core.PageEdit
import dev.vitela.pdf.core.PageSize
import kotlin.math.floor

/**
 * The open Organize grid: a grid of thumbnails standing in for the reader.
 *
 * A card is the page, not the position: [thumbnails] is keyed by position but
 * every edit remaps it ([remapAfterEdit]), so a moved page keeps its picture
 * and only a turned one is rendered again.
 */
data class OrganizeState(
    /** True while an edit is with the core, so a second one cannot start against a layout it has not confirmed. */
    val busy: Boolean = false,
    val thumbnails: Map<Int, ImageBitmap> = emptyMap(),
    /**
     * Bumped whenever the page layout is re-read. A thumbnail asked of an older
     * layout is dropped when it lands instead of being painted onto a card that,
     * by then, shows a different page.
     */
    val version: Int = 0,
    /** The added PDF whose password the import is waiting for, or null. */
    val importPassword: ImportPasswordPrompt? = null,
    /** What the last import could not bring across exactly, shown until dismissed. */
    val importWarnings: List<String> = emptyList(),
    val view: OrganizeView = OrganizeView.Pages,
    /** The Documents view's cards, as the core last grouped them; re-read after every layout change while it shows. */
    val blocks: List<DocumentBlock> = emptyList(),
)

/** What the Organize grid shows a card for: each page, or each run of pages from the same PDF. */
enum class OrganizeView { Pages, Documents }

/** Longer side of a thumbnail, in pixels. Cheap on purpose: a grid shows many pages at once. */
internal const val THUMBNAIL_BOX_PX = 220

/**
 * How many thumbnails stay decoded. A full document of them is an OOM on a long
 * PDF (~120 KB each); the ones scrolled far away are dropped and re-rendered on return.
 */
internal const val THUMBNAIL_CACHE_LIMIT = 48

/** Density used when the page box is unusable. */
internal const val FALLBACK_THUMBNAIL_DPI = 24

// Wording is the Windows shell's where it has one.
internal const val ORGANIZE_LAST_PAGE = "A document needs at least one page."
internal const val ORGANIZE_NO_SUCH_PAGE = "That page is no longer in the document."
internal const val ORGANIZE_QUARTER_TURNS = "Pages turn a quarter at a time."
internal const val ORGANIZE_NOTHING_TO_DO = "The page is already there."

/**
 * Why [edit] must not reach the core, or null when it may. The core owns the
 * permission question; this is only what the shell can know from the layout.
 * Removing the last page is refused here rather than left to the core, which
 * would accept it: a document with no pages leaves the reader nothing to act
 * on, not even a page to put back.
 */
internal fun organizeRefusal(pageCount: Int, edit: PageEdit): String? = when (edit) {
    is PageEdit.Move -> when {
        !runFits(edit.from, edit.count, pageCount) || !runFits(edit.to, edit.count, pageCount) -> ORGANIZE_NO_SUCH_PAGE
        edit.from == edit.to -> ORGANIZE_NOTHING_TO_DO
        else -> null
    }
    is PageEdit.Rotate -> when {
        !runFits(edit.pageIndex, edit.count, pageCount) -> ORGANIZE_NO_SUCH_PAGE
        edit.deltaDegrees != 90 && edit.deltaDegrees != -90 -> ORGANIZE_QUARTER_TURNS
        else -> null
    }
    is PageEdit.Remove -> when {
        !runFits(edit.pageIndex, edit.count, pageCount) -> ORGANIZE_NO_SUCH_PAGE
        edit.count >= pageCount -> ORGANIZE_LAST_PAGE
        else -> null
    }
    // pageCount itself is a valid position: after the last page.
    is PageEdit.InsertBlank -> if (edit.index !in 0..pageCount) ORGANIZE_NO_SUCH_PAGE else null
}

/** Whether the [count] pages starting at [start] are all in a document of [pageCount] pages. */
private fun runFits(start: Int, count: Int, pageCount: Int): Boolean = count >= 1 && start >= 0 && start <= pageCount - count

/**
 * The move that takes the page at [index] one step toward the front ([delta]
 * -1) or back (+1), or null at either end. The core's target is where the page
 * sits *afterwards*, so a neighbour step is simply the neighbour's position.
 */
internal fun neighbourMove(index: Int, delta: Int, pageCount: Int): PageEdit.Move? {
    val to = index + delta
    return if (index in 0 until pageCount && to in 0 until pageCount && to != index) PageEdit.Move(index, to) else null
}

/** Re-keys [byPage] (something cached by page position) for the layout [edit] produces. */
internal fun <T> remapAfterEdit(byPage: Map<Int, T>, edit: PageEdit): Map<Int, T> = when (edit) {
    is PageEdit.Move -> byPage.mapKeys { (position, _) -> positionAfterMove(position, edit.from, edit.to, edit.count) }
    // Only these pages look different now; keeping the old picture would let a
    // stale upright page stand in for the turned one while it renders.
    is PageEdit.Rotate -> byPage.filterKeys { it !in edit.pageIndex until edit.pageIndex + edit.count }
    is PageEdit.Remove -> byPage
        .filterKeys { it !in edit.pageIndex until edit.pageIndex + edit.count }
        .mapKeys { (position, _) -> if (position >= edit.pageIndex + edit.count) position - edit.count else position }
    // The blank card starts without a picture; every page from its position on steps back one.
    is PageEdit.InsertBlank -> byPage.mapKeys { (position, _) -> if (position >= edit.index) position + 1 else position }
}

/** Where [position] sits after the [count] pages at [from] move to [to]; the pages they skip over close up behind them. */
private fun positionAfterMove(position: Int, from: Int, to: Int, count: Int): Int = when {
    position in from until from + count -> to + (position - from)
    from < to && position in (from + count) until (to + count) -> position - count
    from > to && position in to until from -> position + count
    else -> position
}

internal fun organizeStatus(edit: PageEdit): String {
    val done = when (edit) {
        is PageEdit.Move -> pagesDone(edit.count, "moved")
        is PageEdit.Rotate -> pagesDone(edit.count, "rotated")
        is PageEdit.Remove -> pagesDone(edit.count, "deleted")
        is PageEdit.InsertBlank -> "Blank page added."
    }
    return "$done Changes are pending save."
}

private fun pagesDone(count: Int, verb: String): String = if (count == 1) "Page $verb." else "$count pages $verb."


/** The density that fits a page's longer side into [boxPx]. */
internal fun thumbnailDpi(size: PageSize?, boxPx: Int): Int {
    if (size == null || size.widthPt <= 0.0 || size.heightPt <= 0.0) return FALLBACK_THUMBNAIL_DPI
    val longerSidePt = maxOf(size.widthPt, size.heightPt)
    return floor(boxPx * 72.0 / longerSidePt).toInt().coerceIn(MIN_RENDER_DPI, MAX_RENDER_DPI)
}

/** Keeps the [limit] thumbnails nearest [around], the card the user is looking at. */
internal fun <T> trimThumbnails(thumbnails: Map<Int, T>, around: Int, limit: Int): Map<Int, T> {
    if (thumbnails.size <= limit) return thumbnails
    val kept = thumbnails.keys.sortedWith(compareBy({ kotlin.math.abs(it - around) }, { it })).take(limit).toSet()
    return thumbnails.filterKeys { it in kept }
}

/** The annotation toolbar while the grid hides the reader: nothing to draw on, but Undo and Redo still reach the shared log. */
internal fun AnnotationControls.whileOrganizing(): AnnotationControls =
    AnnotationControls.disabled.copy(canUndo = canUndo, canRedo = canRedo)
