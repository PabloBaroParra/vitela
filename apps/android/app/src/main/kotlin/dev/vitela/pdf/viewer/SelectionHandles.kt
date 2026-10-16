package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import kotlin.math.hypot

/** The two ends of a text selection a finger can drag. */
enum class SelectionHandle { Start, End }

/**
 * Where one handle of a selection sits.
 *
 * [caretPoint] is in page space: the mid-height of the line edge the handle
 * marks, so a caret lookup there lands on the selection's own line. The rest
 * are drawn-page pixels: the stem runs from [stemTop] to [foot] along that
 * edge, and the round [knob] the finger grabs hangs under the [foot].
 */
internal data class HandleSpot(
    val handle: SelectionHandle,
    val caretPoint: AnnotationPoint,
    val stemTop: AnnotationPoint,
    val foot: AnnotationPoint,
    val knob: AnnotationPoint,
)

/**
 * The selection's two handles: the start at the first line's left edge, the
 * end at the last line's right edge — the core lists line rects in reading
 * order. A selection without rects has none.
 */
internal fun selectionHandleSpots(selection: TextSelection, placement: PagePlacement, knobRadiusPx: Double): List<HandleSpot> {
    val first = selection.rects.firstOrNull() ?: return emptyList()
    val last = selection.rects.last()
    fun spot(handle: SelectionHandle, x: Double, bottom: Double, height: Double): HandleSpot {
        val foot = placement.placePoint(AnnotationPoint(x, bottom))
        return HandleSpot(
            handle,
            caretPoint = AnnotationPoint(x, bottom + height / 2),
            stemTop = placement.placePoint(AnnotationPoint(x, bottom + height)),
            foot = foot,
            knob = AnnotationPoint(foot.x, foot.y + knobRadiusPx),
        )
    }
    return listOf(
        spot(SelectionHandle.Start, first.x, first.y, first.height),
        spot(SelectionHandle.End, last.x + last.width, last.y, last.height),
    )
}

/** The handle whose knob a touch at drawn-page ([x], [y]) grabs: the nearest within [reachPx], if any. */
internal fun handleAt(spots: List<HandleSpot>, x: Double, y: Double, reachPx: Double): SelectionHandle? =
    spots.map { it to hypot(it.knob.x - x, it.knob.y - y) }
        .filter { (_, distance) -> distance <= reachPx }
        .minByOrNull { (_, distance) -> distance }
        ?.first?.handle

/**
 * Where the selection toolbar's top-left goes on the drawn page, in pixels:
 * [gapPx] above the start handle's line, left-aligned to it; below the end
 * knob when there is no room above. Always kept inside the page's width, and
 * pinned to its left edge when wider. Null when there are no handles.
 */
internal fun selectionToolbarOrigin(
    spots: List<HandleSpot>,
    toolbarWidthPx: Double,
    toolbarHeightPx: Double,
    pageWidthPx: Double,
    pageHeightPx: Double,
    gapPx: Double,
    knobRadiusPx: Double,
): AnnotationPoint? {
    val start = spots.firstOrNull { it.handle == SelectionHandle.Start } ?: return null
    val end = spots.firstOrNull { it.handle == SelectionHandle.End } ?: return null
    val above = minOf(start.stemTop.y, end.stemTop.y) - gapPx - toolbarHeightPx
    val y = if (above >= 0.0) above else minOf(maxOf(start.knob.y, end.knob.y) + knobRadiusPx + gapPx, pageHeightPx - toolbarHeightPx)
    val x = start.foot.x.coerceAtMost(pageWidthPx - toolbarWidthPx).coerceAtLeast(0.0)
    return AnnotationPoint(x, y)
}
