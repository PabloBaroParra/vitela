package dev.vitela.pdf.viewer

import androidx.compose.ui.geometry.Offset
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ReaderZoomTest {
    @Test
    fun pinchFactor_scalesTheStartingZoomContinuouslyWithinTheClamp() {
        assertEquals(1.5, pinchZoomFactor(1.0, 1.5f), 1e-6)
        assertEquals(0.5, pinchZoomFactor(2.0, 0.25f), 1e-6)
        assertEquals(MAX_ZOOM_FACTOR, pinchZoomFactor(4.0, 3f), 0.0)
        assertEquals(MIN_ZOOM_FACTOR, pinchZoomFactor(0.25, 0.1f), 0.0)
    }

    @Test
    fun pinchFactor_ignoresADegenerateGesture() {
        assertEquals(1.25, pinchZoomFactor(1.25, 0f), 0.0)
        assertEquals(1.25, pinchZoomFactor(1.25, Float.NaN), 0.0)
        assertEquals(1.25, pinchZoomFactor(1.25, Float.POSITIVE_INFINITY), 0.0)
    }

    @Test
    fun anchoredScroll_keepsTheContentUnderTheFingersInPlace() {
        // Content 300 px in, fingers 100 px into the viewport: content x = 400.
        // Doubled, that point is at 800, so the viewport must start at 700.
        assertEquals(700, anchoredScroll(scroll = 300, from = 100f, to = 100f, ratio = 2.0))
        // Halved, it is at 200, so the viewport starts at 100.
        assertEquals(100, anchoredScroll(scroll = 300, from = 100f, to = 100f, ratio = 0.5))
    }

    @Test
    fun anchoredScroll_followsTheFingersWhenTheyMoved() {
        // Content x = 400, doubled to 800, now held 250 px into the viewport.
        assertEquals(550, anchoredScroll(scroll = 300, from = 100f, to = 250f, ratio = 2.0))
    }

    @Test
    fun anchoredScroll_measuresFromTheContentNotTheCentringMargin() {
        // A column 100 px in from the left: the fingers at 300 are 200 px into it.
        assertEquals(200, anchoredScroll(scroll = 0, from = 300f, to = 200f, ratio = 2.0, lead = 100f))
    }

    @Test
    fun anchoredScroll_leavesClampingToTheScroller() {
        // The list resolves a negative offset by stepping back to earlier pages.
        assertEquals(-200, anchoredScroll(scroll = 0, from = 400f, to = 400f, ratio = 0.5))
    }

    @Test
    fun pinchShift_followsTheFingersWhereTheEdgeIsUnknown() {
        assertEquals(40f, pinchShift(origin = 500f, pan = 40f, scale = 2f, leading = null, size = null, viewport = 1000f), 0f)
    }

    @Test
    fun pinchShift_neverPullsTheLeadingEdgeIntoView() {
        // Edge at 0, scaled about 500 by 2 lands at -500; a 600 px pan would put it at +100.
        assertEquals(500f, pinchShift(origin = 500f, pan = 600f, scale = 2f, leading = 0f, size = null, viewport = 1000f), 0f)
    }

    @Test
    fun pinchShift_neverPullsTheTrailingEdgeIntoView() {
        // 1000 px of content doubled: its right edge may go no further left than the viewport's.
        assertEquals(-500f, pinchShift(origin = 500f, pan = -900f, scale = 2f, leading = 0f, size = 1000f, viewport = 1000f), 0f)
    }

    @Test
    fun pinchShift_centresContentNarrowerThanTheViewport() {
        // Halved about x = 100, the 1000 px column would start at 50; centred it starts at 250.
        assertEquals(200f, pinchShift(origin = 100f, pan = 0f, scale = 0.5f, leading = 0f, size = 1000f, viewport = 1000f), 0f)
    }

    private fun anchor(fromZoom: Double, toZoom: Double) =
        PinchAnchor(fromZoom, toZoom, firstItem = 0, firstItemOffset = 0, scrollX = 0, leadX = 0f, from = Offset.Zero, to = Offset.Zero)

    /** A pinch from [laidOut] to [target] whose fingers have lifted, its landing [anchor] handed over. */
    private fun lifted(laidOut: Double, target: Double): PinchState = PinchState().apply {
        assertEquals(laidOut, begin(laidOut), 0.0)
        update(target, Offset(100f, 100f), Offset(5f, 5f))
        assertEquals(target, release()!!, 0.0)
        land(anchor(laidOut, target))
    }

    @Test
    fun landing_waitsForTheZoomItAsked_landsOnIt_andDropsAnyOther() {
        val pending = anchor(fromZoom = 1.0, toZoom = 2.0)
        assertEquals(PinchLanding.Wait, pending.landingAt(1.0))
        assertEquals(PinchLanding.Land, pending.landingAt(2.0))
        // The zoom moved somewhere the pinch never asked for: it can never land.
        assertEquals(PinchLanding.Drop, pending.landingAt(1.46))
    }

    @Test
    fun landing_ofAPinchBackToTheSameZoomLandsAtOnce() {
        assertEquals(PinchLanding.Land, anchor(fromZoom = 1.5, toZoom = 1.5).landingAt(1.5))
    }

    @Test
    fun liftedPinch_keepsItsPictureOnlyWhileItsLandingIsPending() {
        val pinch = lifted(laidOut = 1.46, target = 0.25)
        assertEquals((0.25 / 1.46).toFloat(), pinch.scaleOver(1.46), 1e-6f)
        // Laid out at the asked zoom the picture is the page itself.
        assertEquals(1f, pinch.scaleOver(0.25), 0f)
    }

    @Test
    fun liftedPinch_whoseZoomWentElsewhereIsNeverDrawnScaled() {
        // The bug report: 146% laid out, the page drawn at a fifth of it, long after the fingers lifted.
        val pinch = lifted(laidOut = 1.0, target = 0.25)
        assertEquals(1f, pinch.scaleOver(1.46), 0f)
    }

    @Test
    fun settledPinch_stopsScaling() {
        val pinch = lifted(laidOut = 1.0, target = 2.0)
        pinch.settle()
        assertNull(pinch.target)
        assertNull(pinch.pending)
        assertEquals(1f, pinch.scaleOver(1.0), 0f)
        assertEquals(Offset.Zero, pinch.shift)
    }

    @Test
    fun pinchWithoutALanding_isNotDrawnScaledOnceTheFingersLift() {
        val pinch = PinchState()
        pinch.begin(1.0)
        pinch.update(3.0, Offset(10f, 10f), Offset.Zero)
        assertEquals(3f, pinch.scaleOver(1.0), 0f)
        pinch.release()
        assertEquals(1f, pinch.scaleOver(1.0), 0f)
    }

    @Test
    fun abandonedPinch_leavesNothingScaled() {
        // The gesture was torn down with the fingers still on the glass: no lift, nothing to commit.
        val pinch = PinchState()
        pinch.begin(1.0)
        pinch.update(0.3, Offset(10f, 10f), Offset(4f, 4f))
        pinch.abandon()
        assertNull(pinch.target)
        assertEquals(1f, pinch.scaleOver(1.0), 0f)
    }

    @Test
    fun pinchStartedBeforeTheLastOneLanded_startsFromTheZoomOnScreen() {
        val pinch = lifted(laidOut = 1.0, target = 2.0)
        // The list still shows 1.0 laid out, scaled to look like 2.0: the new pinch continues from 2.0.
        assertEquals(2.0, pinch.begin(1.0), 0.0)
    }

    @Test
    fun pinchAfterALiftWithNoLanding_startsFromTheLaidOutZoom() {
        val pinch = PinchState()
        pinch.begin(1.0)
        pinch.update(3.0, Offset(10f, 10f), Offset.Zero)
        pinch.release()
        // Nothing is waiting to land, so the 3.0 left behind is not what the list shows.
        assertEquals(1.0, pinch.begin(1.0), 0.0)
    }

    @Test
    fun landingUnderALivePinch_keepsThatPinchsPicture() {
        val pinch = lifted(laidOut = 1.0, target = 2.0)
        pinch.begin(1.0)
        pinch.update(3.0, Offset(10f, 10f), Offset.Zero)
        pinch.settle()
        assertNull(pinch.pending)
        assertEquals(1.5f, pinch.scaleOver(2.0), 1e-6f)
    }
}
