package dev.vitela.pdf.viewer

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitTouchSlopOrCancellation
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.drag
import androidx.compose.ui.Modifier
import androidx.compose.ui.hapticfeedback.HapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentSelection

/** A drag on a page, from where the finger went down to where it is now, in PDF points. */
internal class PageDrag(val from: AnnotationPoint, val to: AnnotationPoint)

/**
 * The marquee and long-press half of multi-selection (B25 T-208): a long-press
 * then a drag rubber-bands a box, reported live to [onPreview] and, on lifting,
 * to [onSelect]; a long-press that never leaves the touch slop is a press on the
 * item under it, reported to [onPress]. Both are the Android text-selection
 * gesture's shape, which a plain drag — the list's scroll — never reaches: it
 * passes the touch slop before the long-press fires.
 *
 * The page's own tap detector is given an empty long-press while this is
 * [enabled] (see `PageSlot`), so the finger lifting is not also a tap. Not
 * covered by the JVM tests, which have no pointer input; the decisions it makes
 * are the pure functions in `ContentSelections.kt`.
 */
internal fun Modifier.marqueeGesture(
    enabled: Boolean,
    placement: PagePlacement,
    haptics: HapticFeedback,
    onPreview: (PageDrag?) -> Unit,
    onSelect: (AnnotationRect) -> Unit,
    onPress: (AnnotationPoint) -> Unit,
): Modifier = pointerInput(enabled, placement) {
    if (!enabled) return@pointerInput
    var origin: AnnotationPoint? = null
    var end: AnnotationPoint? = null
    // A touch slop in screen pixels, as the points of this page.
    val slop = viewConfiguration.touchSlop / (placement.scale.takeIf { it.isFinite() && it > 0.0 } ?: 1.0)
    detectDragGesturesAfterLongPress(
        onDragStart = { offset ->
            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
            val start = placement.pointToPdf(offset.x.toDouble(), offset.y.toDouble())
            origin = start
            end = start
            onPreview(PageDrag(start, start))
        },
        onDrag = { change, _ ->
            change.consume()
            val start = origin ?: return@detectDragGesturesAfterLongPress
            val now = placement.pointToPdf(change.position.x.toDouble(), change.position.y.toDouble())
            end = now
            onPreview(PageDrag(start, now))
        },
        onDragEnd = {
            val start = origin
            val finish = end
            origin = null
            end = null
            onPreview(null)
            if (start != null && finish != null) {
                if (isPress(start, finish, slop)) onPress(start) else onSelect(marqueeRect(start, finish))
            }
        },
        onDragCancel = {
            origin = null
            end = null
            onPreview(null)
        },
    )
}

/**
 * Dragging the selection: a finger that goes down inside its dashed box — or
 * within [reach] points of it — and moves past the touch slop carries every
 * member, reported live to [onPreview], and lifting sends the offset to
 * [onDrop]. A drag that starts anywhere else is left to the list's scroll.
 *
 * A finger that has been down as long as a long-press takes is the marquee's,
 * not this one's, so it is never claimed here: holding inside the box and then
 * dragging starts a new marquee, like anywhere else. Declared after
 * [marqueeGesture], so it sees the touch first.
 */
internal fun Modifier.dragSelection(
    selection: ContentSelection?,
    placement: PagePlacement,
    reach: Double,
    onPreview: (PageDrag?) -> Unit,
    onDrop: (dx: Double, dy: Double) -> Unit,
): Modifier = pointerInput(selection, placement, reach) {
    val box = selection?.bounds ?: return@pointerInput
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false)
        val from = placement.pointToPdf(down.position.x.toDouble(), down.position.y.toDouble())
        if (!grabs(box, from, reach)) return@awaitEachGesture
        val slopChange = awaitTouchSlopOrCancellation(down.id) { change, _ ->
            if (change.uptimeMillis - down.uptimeMillis < viewConfiguration.longPressTimeoutMillis) change.consume()
        } ?: return@awaitEachGesture
        var to = placement.pointToPdf(slopChange.position.x.toDouble(), slopChange.position.y.toDouble())
        onPreview(PageDrag(from, to))
        val completed = drag(slopChange.id) { change ->
            change.consume()
            to = placement.pointToPdf(change.position.x.toDouble(), change.position.y.toDouble())
            onPreview(PageDrag(from, to))
        }
        onPreview(null)
        if (completed) onDrop(to.x - from.x, to.y - from.y)
    }
}
