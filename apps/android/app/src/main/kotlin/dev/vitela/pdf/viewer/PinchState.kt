package dev.vitela.pdf.viewer

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset

/**
 * How far a pinch-out may shrink the picture of the zoom laid out before that
 * zoom is asked to be laid out for real. What the shrunken picture reveals is
 * already laid out up to [READER_OVERSCAN] past the screen, which covers a
 * picture down to two thirds whatever the fingers hold
 * ([coveredPictureScale]); the step has to be asked early enough that the
 * picture is still above that when it lands, a frame or two later. The
 * fastest pinch measured on a phone shrinks to about 0.78 in that time
 * ([PINCH_STEP_INTERVAL_MS]), and 0.9 × 0.78 ≈ 0.70 keeps clear of 0.67. A
 * higher ratio only buys relayouts: a pinch from 400 % down to fit width takes
 * about thirteen at 0.9.
 */
internal const val PINCH_STEP_RATIO = 0.9

/**
 * The least time between two pinch-out relayouts: two frames at 60 Hz. A step
 * lays out in about one frame and steps already go one at a time, so this only
 * keeps a fast pinch from spending every frame on layout. It must stay short:
 * the picture keeps shrinking over the old layout for this long plus the
 * landing, which is what [PINCH_STEP_RATIO] and [READER_OVERSCAN] are sized
 * against — at the 80 ms this once was, a fast pinch reached 0.54.
 */
internal const val PINCH_STEP_INTERVAL_MS = 32L

/** A picture of the reader: [scale]d about [origin], then moved by [translation]. */
internal data class PinchPicture(val scale: Float, val origin: Offset, val translation: Offset) {
    /** Where the laid-out point [point] is drawn. */
    fun map(point: Offset): Offset = origin + translation + (point - origin) * scale

    /** The laid-out point drawn at [drawn]. */
    fun unmap(drawn: Offset): Offset = origin + (drawn - origin - translation) / scale

    companion object {
        val None = PinchPicture(1f, Offset.Zero, Offset.Zero)
    }
}

/** A layout a pinch can be drawn over: laid out at [zoom], it has the content under the fingers at [at]. */
internal data class PinchBase(val zoom: Double, val at: Offset)

/**
 * A pinch in progress, or lifted and waiting to land. While the fingers are
 * down the reader is scaled as a picture — relaying out and re-rasterizing
 * every page on each move would stutter — and the real zoom is committed when
 * they lift. A pinch-out also commits [step]s while the fingers are still
 * down: a picture smaller than the reader shows what is laid out around the
 * screen, and that overscan only reaches so far.
 *
 * The picture is scaled by [target] over the zoom actually laid out, so the
 * frame in which a committed zoom lays out is also the frame the picture
 * rescales: no flash at the old size, and no frame scaled twice. Which layout
 * is on screen is known only to the layout pass, so the state keeps one
 * [PinchBase] per layout it may be drawn over — the one laid out and the one a
 * pending step will lay out — and [picture] picks by the zoom laid out.
 *
 * It scales about where the fingers first met, [origin], and follows them by
 * [shift]: moving the scale's origin with the fingers instead would slide the
 * picture by every wobble.
 *
 * The picture is only ever scaled while fingers are pinching or while the
 * [pending] landing can still land on the zoom laid out. Anything else — a
 * landing whose zoom went elsewhere, a gesture torn down before its fingers
 * lifted — draws the pages as laid out, never a stale scaled picture.
 */
internal class PinchState {
    /** The zoom the fingers are asking for; null when no pinch is showing. */
    var target by mutableStateOf<Double?>(null)
        private set
    var origin by mutableStateOf(Offset.Zero)
        private set
    var shift by mutableStateOf(Offset.Zero)
        private set

    /** Fingers are on the glass, pinching. */
    var live by mutableStateOf(false)
        private set

    /** Where a lifted pinch or a step is to land; null when none is waiting. */
    var pending by mutableStateOf<PinchAnchor?>(null)
        private set

    /** The fingers lifted while a step was still landing: the release commits once it has. */
    var releaseDeferred by mutableStateOf(false)
        private set

    private var bases by mutableStateOf(emptyList<PinchBase>())
    private var lastStepAt: Long? = null

    private fun showing(laidOut: Double): Boolean =
        target != null && (live || releaseDeferred || pending?.landingAt(laidOut) == PinchLanding.Wait)

    private fun pictureOver(base: PinchBase): PinchPicture {
        val target = target ?: return PinchPicture.None
        return PinchPicture((target / base.zoom).toFloat(), base.at, origin + shift - base.at)
    }

    /** How the reader is drawn while [laidOut] is the zoom laid out. */
    fun picture(laidOut: Double): PinchPicture {
        if (!showing(laidOut)) return PinchPicture.None
        val base = bases.firstOrNull { it.zoom == laidOut } ?: return PinchPicture.None
        return pictureOver(base)
    }

    /**
     * Fingers met at [origin]: the zoom the new pinch scales from. While an
     * earlier pinch is still showing, the list looks like its [target], not
     * [laidOut], so the new one carries on from there without moving the
     * picture.
     */
    fun begin(laidOut: Double, origin: Offset): Double {
        val carried = if (showing(laidOut)) {
            bases.map { base -> PinchBase(base.zoom, pictureOver(base).unmap(origin)) }
        } else emptyList()
        val frame = if (carried.isEmpty()) laidOut else target!!
        bases = if (carried.any { it.zoom == frame }) carried else carried + PinchBase(frame, origin)
        this.origin = origin
        target = frame
        shift = Offset.Zero
        live = true
        releaseDeferred = false
        lastStepAt = null
        return frame
    }

    fun update(target: Double, shift: Offset) {
        this.target = target
        this.shift = shift
    }

    /**
     * Whether a pinch-out has shrunk the picture of [laidOut] far enough, and
     * long enough after the last step, to lay out its zoom now. One step at a
     * time: a step taken before the last one is laid out would be anchored to
     * a layout that never reaches the screen.
     */
    fun stepDue(laidOut: Double, nowMs: Long): Boolean {
        val target = target ?: return false
        if (!live || pending != null || bases.none { it.zoom == laidOut }) return false
        if (target >= laidOut * PINCH_STEP_RATIO) return false
        return lastStepAt?.let { nowMs - it >= PINCH_STEP_INTERVAL_MS } ?: true
    }

    /**
     * The content under the fingers in the [laidOut] layout, and where the
     * fingers hold it now: what a step or a release anchors to. Null over a
     * layout the pinch knows nothing about.
     */
    fun landingPoints(laidOut: Double): Pair<Offset, Offset>? =
        bases.firstOrNull { it.zoom == laidOut }?.let { it.at to origin + shift }

    /** The fingers are still down, but [anchor]'s zoom is being laid out now. */
    fun step(anchor: PinchAnchor, nowMs: Long) {
        pending = anchor
        lastStepAt = nowMs
        bases = bases.filter { it.zoom != anchor.toZoom } + PinchBase(anchor.toZoom, anchor.to)
    }

    /**
     * The fingers lifted: the zoom to commit, or null when there is none —
     * including while a step is still landing, when it is deferred until
     * [settle] says it is due.
     */
    fun release(): Double? {
        live = false
        val target = target ?: return null
        if (pending != null) {
            releaseDeferred = true
            return null
        }
        return target
    }

    /** The gesture was torn down before its fingers lifted: nothing commits, so nothing may stay scaled. */
    fun abandon() {
        live = false
        clear()
    }

    /** The lifted pinch is to land at [anchor] once its zoom is laid out. */
    fun land(anchor: PinchAnchor) {
        pending = anchor
    }

    /**
     * The pending landing [landed] (true) or can no longer land (false). The
     * list may have stopped short of where the anchor asked — at its end, or
     * centring a narrow column — so [landedAt] is where the content under the
     * fingers actually is; the picture makes up the difference. A pinch still
     * under the fingers keeps its picture. True when a deferred release is due
     * now: the caller lands it from the layout just landed.
     */
    fun settle(landed: Boolean, landedAt: Offset?): Boolean {
        val anchor = pending
        pending = null
        val due = releaseDeferred && landed
        releaseDeferred = false
        bases = if (landed && anchor != null) {
            val drift = landedAt?.let { it - anchor.to } ?: Offset.Zero
            bases.filter { it.zoom == anchor.toZoom }.map { PinchBase(it.zoom, it.at + drift) }
        } else emptyList()
        if (!live && !due) clear()
        return due
    }

    fun clear() {
        target = null
        shift = Offset.Zero
        pending = null
        releaseDeferred = false
        bases = emptyList()
    }
}

/** What a layout pass does with a lifted pinch's [PinchAnchor]. */
internal enum class PinchLanding {
    /** The committed zoom is not laid out yet. */
    Wait,

    /** The committed zoom is laid out: scroll back under the fingers. */
    Land,

    /** The zoom laid out is neither where the pinch started nor where it asked to go: it can never land. */
    Drop,
}

/**
 * The list's position, in the zoom it was taken at, when a pinch lifted or
 * stepped: the content under [from] is to end up under [to] once [toZoom] is
 * laid out.
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
) {
    fun landingAt(laidOut: Double): PinchLanding = when (laidOut) {
        toZoom -> PinchLanding.Land
        fromZoom -> PinchLanding.Wait
        else -> PinchLanding.Drop
    }
}

/**
 * Where the content [anchor] held under its fingers actually is once its zoom
 * is laid out: [itemTop] is where its first item now starts (null when that
 * item is off screen, and the anchor is trusted), [scrollX] the horizontal
 * scroll and [lead] the centring margin the new column got.
 */
internal fun landedPoint(anchor: PinchAnchor, itemTop: Int?, scrollX: Int, lead: Float): Offset {
    val ratio = anchor.toZoom / anchor.fromZoom
    val x = lead - scrollX + ratio * (anchor.scrollX + anchor.from.x - anchor.leadX)
    val y = itemTop?.let { it + ratio * (anchor.firstItemOffset + anchor.from.y) } ?: anchor.to.y.toDouble()
    return Offset(x.toFloat(), y.toFloat())
}
