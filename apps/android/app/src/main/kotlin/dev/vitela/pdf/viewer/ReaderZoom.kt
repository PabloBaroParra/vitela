package dev.vitela.pdf.viewer

import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.layout
import kotlin.math.roundToInt

/** The reader's zoom controls: the ladder buttons, the two fits, and where a pinch lands. */
internal class ZoomActions(
    val onZoomIn: () -> Unit,
    val onZoomOut: () -> Unit,
    val onFitWidth: () -> Unit,
    val onFitPage: () -> Unit,
    val onPinch: (zoomFactor: Double) -> Unit,
)

/** The zoom a pinch of [gestureScale] lands on from [startFactor]: continuous, unlike the buttons' ladder. */
internal fun pinchZoomFactor(startFactor: Double, gestureScale: Float): Double {
    if (!gestureScale.isFinite() || gestureScale <= 0f) return startFactor
    return clampZoomFactor(startFactor * gestureScale)
}

/**
 * The scroll offset that puts the content that was under [from] (viewport
 * pixels, [scroll] in, the content starting [lead] pixels into the viewport)
 * under [to] once the content has grown by [ratio]. Not clamped: the scroller
 * applying it knows its own bounds.
 */
internal fun anchoredScroll(scroll: Int, from: Float, to: Float, ratio: Double, lead: Float = 0f): Int =
    ((scroll + from - lead) * ratio - to).roundToInt()

/**
 * How far a pinch picture scaled by [scale] about [origin] is shifted along one
 * axis: the fingers' [pan], held to where the committed zoom can actually land.
 * [leading] is where the content starts in the viewport, null when unknown;
 * [size] its length, null when unknown. Content shorter than the [viewport]
 * is centred, as the laid-out reader centres it.
 */
internal fun pinchShift(origin: Float, pan: Float, scale: Float, leading: Float?, size: Float?, viewport: Float): Float {
    if (leading == null) return pan
    val start = origin + pan + scale * (leading - origin)
    val visual = size?.let { it * scale }
    val wanted = when {
        visual == null -> minOf(start, 0f)
        visual <= viewport -> (viewport - visual) / 2
        else -> start.coerceIn(viewport - visual, 0f)
    }
    return pan + (wanted - start)
}

/** Where the reader's content sat when a pinch began, for [pinchShift]. */
internal class PinchEdges(val left: Float, val width: Float, val top: Float?)

/**
 * A pinch in progress. While the fingers are down the reader is only scaled as
 * a picture — relaying out and re-rasterizing every page on each move would
 * stutter — and the real zoom is committed once, when they lift.
 *
 * The picture is scaled by [target] over the zoom actually laid out, not by
 * the raw gesture, so the frame in which the committed zoom lays out is also
 * the frame the picture stops being scaled: no flash at the old size, and no
 * frame scaled twice.
 *
 * It scales about where the fingers first met, [origin], and follows them by
 * [shift]: moving the scale's origin with the fingers instead would slide the
 * picture by every wobble.
 */
internal class PinchState {
    /** The zoom the fingers are asking for; null when no pinch is showing. */
    var target by mutableStateOf<Double?>(null)
        private set
    var origin by mutableStateOf(Offset.Zero)
        private set
    var shift by mutableStateOf(Offset.Zero)
        private set

    fun scaleOver(laidOut: Double): Float = ((target ?: laidOut) / laidOut).toFloat()

    fun update(target: Double, origin: Offset, shift: Offset) {
        this.target = target
        this.origin = origin
        this.shift = shift
    }

    fun clear() {
        target = null
        shift = Offset.Zero
    }
}

/**
 * The list's position, in the zoom it was taken at, when a pinch lifted: the
 * content under [from] is to end up under [to] once [toZoom] is laid out.
 */
internal class PinchAnchor(
    val fromZoom: Double,
    val toZoom: Double,
    val firstItem: Int,
    val firstItemOffset: Int,
    val scrollX: Int,
    val leadX: Float,
    val from: Offset,
    val to: Offset,
)

/**
 * Two fingers zoom; one finger is left alone for the list's scroll, the page
 * tools and text selection. Listening on the initial pass is what lets the
 * pinch claim a two-finger move before the list underneath scrolls with it.
 * [onPinchEnd] gets the zoom to commit, where the content under the fingers
 * started and where it is now; clearing [pinch] once that lands is the caller's.
 */
internal fun Modifier.pinchToZoom(
    enabled: Boolean,
    laidOutZoom: () -> Double,
    edges: () -> PinchEdges,
    pinch: PinchState,
    onPinchEnd: (target: Double, from: Offset, to: Offset) -> Unit,
): Modifier =
    pointerInput(enabled) {
        if (!enabled) return@pointerInput
        awaitEachGesture {
            awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            val start = laidOutZoom()
            var gestureScale = 1f
            var pan = Offset.Zero
            var origin: Offset? = null
            var at = PinchEdges(0f, 0f, null)
            do {
                val event = awaitPointerEvent(PointerEventPass.Initial)
                val down = event.changes.filter { it.pressed }
                if (down.size >= 2) {
                    val began = origin == null
                    // The built-in centroid skips a finger that just landed, so average them by hand.
                    val o = origin ?: (down.fold(Offset.Zero) { sum, change -> sum + change.position } / down.size.toFloat())
                        .also { origin = it; at = edges() }
                    if (!began) {
                        gestureScale *= event.calculateZoom()
                        pan += event.calculatePan()
                    }
                    val target = pinchZoomFactor(start, gestureScale)
                    val scale = (target / start).toFloat()
                    val shift = Offset(
                        pinchShift(o.x, pan.x, scale, at.left, at.width, size.width.toFloat()),
                        pinchShift(o.y, pan.y, scale, at.top, null, size.height.toFloat()),
                    )
                    pinch.update(target, o, shift)
                }
                // Once pinching, the finger left behind must not turn into a scroll or a tap either.
                if (origin != null) event.changes.forEach { it.consume() }
            } while (event.changes.any { it.pressed })
            val target = pinch.target
            if (origin != null && target != null) onPinchEnd(target, pinch.origin, pinch.origin + pinch.shift)
        }
    }
        .graphicsLayer {
            val scale = pinch.scaleOver(laidOutZoom())
            scaleX = scale
            scaleY = scale
            translationX = pinch.shift.x
            translationY = pinch.shift.y
            transformOrigin = if (size.width > 0f && size.height > 0f) {
                TransformOrigin(pinch.origin.x / size.width, pinch.origin.y / size.height)
            } else TransformOrigin.Center
            clip = true
        }

/**
 * Lands a lifted pinch in the very layout pass that lays out its zoom. Waiting
 * a frame to scroll would show one frame at the new zoom but the old scroll —
 * the jump on release. The list's position is requested before it measures;
 * the horizontal one is set once the row is measured, since the scroller
 * clamps to its old width until then. Goes before the horizontal scroller.
 */
internal fun Modifier.landPinch(
    anchor: () -> PinchAnchor?,
    laidOutZoom: () -> Double,
    list: LazyListState,
    horizontal: ScrollState,
    onLanded: () -> Unit,
): Modifier =
    layout { measurable, constraints ->
        val landing = anchor()?.takeIf { it.toZoom == laidOutZoom() }
        val ratio = landing?.let { it.toZoom / it.fromZoom } ?: 1.0
        if (landing != null) {
            list.requestScrollToItem(
                landing.firstItem,
                anchoredScroll(landing.firstItemOffset, landing.from.y, landing.to.y, ratio),
            )
        }
        val placeable = measurable.measure(constraints)
        layout(placeable.width, placeable.height) {
            if (landing != null) {
                val x = anchoredScroll(landing.scrollX, landing.from.x, landing.to.x, ratio, landing.leadX)
                horizontal.dispatchRawDelta((x - horizontal.value).toFloat())
                onLanded()
            }
            placeable.place(0, 0)
        }
    }
