package dev.vitela.pdf.viewer

import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
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
    /** A pinch commits a zoom: [live] while its fingers are still down, so the reader holds off re-rasterizing. */
    val onPinch: (zoomFactor: Double, live: Boolean) -> Unit,
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
internal class PinchEdges(val left: Float, val width: Float, val top: Float?) {
    /** The same edges as drawn by [picture]: a pinch begun over a picture measures what is on screen. */
    fun through(picture: PinchPicture): PinchEdges {
        val leading = picture.map(Offset(left, top ?: 0f))
        return PinchEdges(leading.x, width * picture.scale, top?.let { leading.y })
    }
}

/**
 * Two fingers zoom; one finger is left alone for the list's scroll, the page
 * tools and text selection. Listening on the initial pass is what lets the
 * pinch claim a two-finger move before the list underneath scrolls with it.
 *
 * While the fingers pinch out, [onStep] is asked to lay out the zoom they are
 * at whenever [PinchState.stepDue] says the picture has shrunk enough; it gets
 * the zoom and the event time, and hands [pinch] the step. [onRelease] gets
 * the zoom to commit once they lift (unless a step is still landing, when
 * [pinch] defers it). [onAbandon] runs when a gesture that stepped is torn
 * down before its fingers lifted, so the reader stops holding off renders.
 *
 * The picture is clipped to the reader's own bounds: a layer's own clip is
 * scaled with it, which let a zoomed-in picture draw over the bars around it.
 */
internal fun Modifier.pinchToZoom(
    enabled: Boolean,
    laidOutZoom: () -> Double,
    edges: () -> PinchEdges,
    pinch: PinchState,
    onStep: (target: Double, nowMs: Long) -> Unit,
    onAbandon: () -> Unit,
    onRelease: (target: Double) -> Unit,
): Modifier =
    pointerInput(enabled) {
        if (!enabled) return@pointerInput
        awaitEachGesture {
            awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            var frame = 1.0
            var gestureScale = 1f
            var pan = Offset.Zero
            var origin: Offset? = null
            var at = PinchEdges(0f, 0f, null)
            var lifted = false
            var stepped = false
            try {
                do {
                    val event = awaitPointerEvent(PointerEventPass.Initial)
                    val down = event.changes.filter { it.pressed }
                    if (down.size >= 2) {
                        val began = origin == null
                        // The built-in centroid skips a finger that just landed, so average them by hand.
                        val o = origin ?: (down.fold(Offset.Zero) { sum, change -> sum + change.position } / down.size.toFloat())
                            .also {
                                origin = it
                                at = edges().through(pinch.picture(laidOutZoom()))
                                frame = pinch.begin(laidOutZoom(), it)
                            }
                        if (!began) {
                            gestureScale *= event.calculateZoom()
                            pan += event.calculatePan()
                        }
                        val target = pinchZoomFactor(frame, gestureScale)
                        // Against the frame the gesture began in, not the zoom laid out: steps re-lay the list out under it.
                        val scale = (target / frame).toFloat()
                        val shift = Offset(
                            pinchShift(o.x, pan.x, scale, at.left, at.width, size.width.toFloat()),
                            pinchShift(o.y, pan.y, scale, at.top, null, size.height.toFloat()),
                        )
                        pinch.update(target, shift)
                        val now = down.first().uptimeMillis
                        if (pinch.stepDue(laidOutZoom(), now)) {
                            stepped = true
                            onStep(target, now)
                        }
                    }
                    // Once pinching, the finger left behind must not turn into a scroll or a tap either.
                    if (origin != null) event.changes.forEach { it.consume() }
                } while (event.changes.any { it.pressed })
                lifted = true
            } finally {
                // A reset (key change, detach, density change) cancels the gesture with the fingers still down.
                if (!lifted && origin != null) {
                    pinch.abandon()
                    if (stepped) onAbandon()
                }
            }
            if (origin == null) return@awaitEachGesture
            val target = pinch.release() ?: return@awaitEachGesture
            onRelease(target)
        }
    }
        .clipToBounds()
        .graphicsLayer {
            val picture = pinch.picture(laidOutZoom())
            scaleX = picture.scale
            scaleY = picture.scale
            translationX = picture.translation.x
            translationY = picture.translation.y
            transformOrigin = if (size.width > 0f && size.height > 0f) {
                TransformOrigin(picture.origin.x / size.width, picture.origin.y / size.height)
            } else TransformOrigin.Center
        }

/**
 * Lands a pinch's step or release in the very layout pass that lays out its
 * zoom. Waiting a frame to scroll would show one frame at the new zoom but the
 * old scroll — the jump on release. The list's position is requested before
 * it measures; the horizontal one is set once the row is measured, since the
 * scroller clamps to its old width until then. Goes before the horizontal
 * scroller. [onSettled] runs once the landing landed (with where the content
 * under the fingers actually ended up, [lead] being the new column's centring
 * margin) or can no longer land (with null).
 */
internal fun Modifier.landPinch(
    anchor: () -> PinchAnchor?,
    laidOutZoom: () -> Double,
    list: LazyListState,
    horizontal: ScrollState,
    lead: () -> Float,
    onSettled: (outcome: PinchLanding, landedAt: Offset?) -> Unit,
): Modifier =
    layout { measurable, constraints ->
        val outcome = anchor()?.landingAt(laidOutZoom())
        val landing = anchor()?.takeIf { outcome == PinchLanding.Land }
        val ratio = landing?.let { it.toZoom / it.fromZoom } ?: 1.0
        if (landing != null) {
            list.requestScrollToItem(
                landing.firstItem,
                anchoredScroll(landing.firstItemOffset, landing.from.y, landing.to.y, ratio),
            )
        }
        val placeable = measurable.measure(constraints)
        // The list measured just now, so this is where the landing actually put its first item.
        val itemTop = landing?.let { l -> list.layoutInfo.visibleItemsInfo.firstOrNull { it.index == l.firstItem }?.offset }
        layout(placeable.width, placeable.height) {
            var landedAt: Offset? = null
            if (landing != null) {
                val x = anchoredScroll(landing.scrollX, landing.from.x, landing.to.x, ratio, landing.leadX)
                horizontal.dispatchRawDelta((x - horizontal.value).toFloat())
                landedAt = landedPoint(landing, itemTop, horizontal.value, lead())
            }
            if (outcome == PinchLanding.Land || outcome == PinchLanding.Drop) onSettled(outcome, landedAt)
            placeable.place(0, 0)
        }
    }
