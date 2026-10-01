package dev.vitela.pdf.viewer

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
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
 * The scroll offset that keeps the content under [focus] (viewport pixels)
 * where it was after the content grew by [ratio] from [scroll].
 */
internal fun anchoredScroll(scroll: Int, focus: Float, ratio: Double): Int =
    ((scroll + focus) * ratio - focus).roundToInt().coerceAtLeast(0)

/**
 * A pinch in progress. While the fingers are down the reader is only scaled as
 * a picture — relaying out and re-rasterizing every page on each move would
 * stutter — and the real zoom is committed once, when they lift.
 *
 * The picture is scaled by [target] over the zoom actually laid out, not by
 * the raw gesture, so the frame in which the committed zoom lays out is also
 * the frame the picture stops being scaled: no flash at the old size, and no
 * frame scaled twice.
 */
internal class PinchState {
    /** The zoom the fingers are asking for; null when no pinch is showing. */
    var target by mutableStateOf<Double?>(null)
    var focus by mutableStateOf(Offset.Zero)
        private set

    fun scaleOver(laidOut: Double): Float = ((target ?: laidOut) / laidOut).toFloat()

    fun update(target: Double, centroid: Offset) {
        this.target = target
        focus = centroid
    }
}

/**
 * Two fingers zoom; one finger is left alone for the list's scroll, the page
 * tools and text selection. Listening on the initial pass is what lets the
 * pinch claim a two-finger move before the list underneath scrolls with it.
 * [onPinchEnd] gets the zoom to commit and the point to keep still; clearing
 * [PinchState.target] once that zoom is laid out is the caller's.
 */
internal fun Modifier.pinchToZoom(
    enabled: Boolean,
    laidOutZoom: () -> Double,
    pinch: PinchState,
    onPinchEnd: (target: Double, focus: Offset) -> Unit,
): Modifier =
    pointerInput(enabled) {
        if (!enabled) return@pointerInput
        awaitEachGesture {
            awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            val start = laidOutZoom()
            var gestureScale = 1f
            var pinching = false
            do {
                val event = awaitPointerEvent(PointerEventPass.Initial)
                if (event.changes.count { it.pressed } >= 2) {
                    pinching = true
                    gestureScale *= event.calculateZoom()
                    pinch.update(pinchZoomFactor(start, gestureScale), event.calculateCentroid(useCurrent = true))
                }
                // Once pinching, the finger left behind must not turn into a scroll or a tap either.
                if (pinching) event.changes.forEach { it.consume() }
            } while (event.changes.any { it.pressed })
            pinch.target?.let { target -> if (pinching) onPinchEnd(target, pinch.focus) }
        }
    }
        .graphicsLayer {
            val scale = pinch.scaleOver(laidOutZoom())
            scaleX = scale
            scaleY = scale
            transformOrigin = if (size.width > 0f && size.height > 0f) {
                TransformOrigin(pinch.focus.x / size.width, pinch.focus.y / size.height)
            } else TransformOrigin.Center
            clip = true
        }
