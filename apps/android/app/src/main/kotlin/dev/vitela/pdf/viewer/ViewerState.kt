package dev.vitela.pdf.viewer

import androidx.compose.ui.graphics.ImageBitmap
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.SearchHit
import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.SaveSnapshot

data class ViewerState(
    val title: String = "No document open",
    val pageCount: Int = 0,
    /** The page filling most of the viewport — what the page counter reports. */
    val pageIndex: Int = 0,
    /** Explicit zoom relative to the reader width: 1.0 is the existing fit-to-width layout. */
    val zoomFactor: Double = DEFAULT_ZOOM_FACTOR,
    /** Every page's media box, so unrendered slots still lay out at the right height. */
    val pageSizes: List<PageSize> = emptyList(),
    /**
     * The decoded pages currently resident, keyed by page index. Deliberately
     * sparse: [CACHE_PAGES] bounds how much of a long document may be held at
     * once, and everything outside that window is evicted on scroll.
     */
    val pages: Map<Int, ImageBitmap> = emptyMap(),
    /**
     * Bitmaps from the previous layout generation, drawn beneath replacement
     * pages so a zoom or resize has no blank intermediate frame.
     */
    val bridgePages: Map<Int, ImageBitmap> = emptyMap(),
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
    val annotations: List<Annotation> = emptyList(),
    val annotationEditingAllowed: Boolean = false,
    val selectedAnnotationId: Long? = null,
    val activeAnnotationTool: AnnotationTool = AnnotationTool.Pointer,
    val textSelection: TextSelection? = null,
    val canUndoAnnotations: Boolean = false,
    val canRedoAnnotations: Boolean = false,
    /** True after an edit and until a matching SAF write succeeds. */
    val isDirty: Boolean = false,
    /** Identifies the active document so an older save can never clear newer work. */
    val documentId: Long = 0,
    val revision: Long = 0,
    /**
     * Where **Save** writes back to: an opaque token the shell minted for the
     * file it opened (a SAF URI string), or null when there is nothing
     * writable to go back to — the packaged sample, a read-only provider, or
     * a target a write already failed on. Save copy works either way.
     */
    val saveTarget: String? = null,
    /**
     * The file name of every PDF added from Organize, by the id the core gave
     * its pages ([dev.vitela.pdf.core.ImportReport.sourceId]) — the core knows
     * which pages came from which file, only the shell knows what it was called.
     */
    val importedSourceNames: Map<Long, String> = emptyMap(),
    /** The Document properties dialog, or null while it is closed. */
    val metadataEditor: MetadataEditor? = null,
    /** The Export images dialog, or null while it is closed. */
    val imageExport: ImageExportEditor? = null,
    /** True while an export is rendering and writing pages, so a second one cannot start on top of it. */
    val imageExportRunning: Boolean = false,
    /** The Extract pages dialog, or null while it is closed. */
    val pageExtract: PageExtractEditor? = null,
    /** The Split dialog, or null while it is closed. */
    val pageSplit: PageSplitEditor? = null,
    /** True while a split is extracting and writing its parts, so a second one cannot start on top of it. */
    val pageSplitRunning: Boolean = false,
    /** The Compress dialog, or null while it is closed. */
    val compress: CompressEditor? = null,
    /** True while a compression runs, so a second one cannot start on top of it. */
    val compressRunning: Boolean = false,
    /** The Protect dialog, or null while it is closed. */
    val protect: ProtectEditor? = null,
    /** True while a protection runs, so a second one cannot start on top of it. */
    val protectRunning: Boolean = false,
    /** The Sign dialog, or null while it is closed. */
    val sign: SignEditor? = null,
    /** True while a signature is computed and written, so a second one cannot start on top of it. */
    val signRunning: Boolean = false,
    /** The Organize grid standing in for the reader, or null while the reader shows. */
    val organize: OrganizeState? = null,
    /** The Form fields panel below the reader, or null while it is closed. */
    val formFields: FormFieldsState? = null,
    /** Edit text mode, or null while page taps belong to the annotation tools. */
    val contentEdit: ContentEditState? = null,
    /** A loaded replacement held in the ViewModel pending user confirmation. */
    val pendingReplacementTitle: String? = null,
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
    val zoomFactor: Double,
)

internal fun boundedPageIndex(pageIndex: Int, pageCount: Int): Int =
    pageIndex.coerceIn(0, (pageCount - 1).coerceAtLeast(0))

internal fun nextSearchIndex(current: Int, count: Int, delta: Int): Int {
    if (count == 0) return 0
    return Math.floorMod(current + delta, count)
}

/** A write can clear dirty state only when it still represents this document revision. */
internal fun ViewerState.matches(snapshot: SaveSnapshot): Boolean =
    documentId == snapshot.documentId && revision == snapshot.revision

/**
 * A snapshot bound to the target it must be written to. Taken as one value
 * under the document lane, so a replacement that lands between the tap and
 * the write can never send one document's bytes into another's file.
 */
data class InPlaceSave(val target: String, val snapshot: SaveSnapshot)

internal const val MIN_ZOOM_FACTOR = 0.10
internal const val MAX_ZOOM_FACTOR = 8.0
internal const val DEFAULT_ZOOM_FACTOR = 1.0

private val ZOOM_LADDER = doubleArrayOf(0.10, 0.25, 0.50, 0.75, 1.00, 1.25, 1.50, 2.00, 3.00, 4.00, 6.00, 8.00)
private const val ZOOM_EPSILON = 1e-9

internal fun clampZoomFactor(factor: Double): Double =
    if (factor.isFinite()) factor.coerceIn(MIN_ZOOM_FACTOR, MAX_ZOOM_FACTOR) else DEFAULT_ZOOM_FACTOR

/** Returns the next explicit zoom-in rung, including when the current factor is between rungs. */
internal fun zoomIn(factor: Double): Double {
    val current = clampZoomFactor(factor)
    return ZOOM_LADDER.firstOrNull { it > current + ZOOM_EPSILON } ?: MAX_ZOOM_FACTOR
}

/** Returns the next explicit zoom-out rung, including when the current factor is between rungs. */
internal fun zoomOut(factor: Double): Double {
    val current = clampZoomFactor(factor)
    return ZOOM_LADDER.lastOrNull { it < current - ZOOM_EPSILON } ?: MIN_ZOOM_FACTOR
}
