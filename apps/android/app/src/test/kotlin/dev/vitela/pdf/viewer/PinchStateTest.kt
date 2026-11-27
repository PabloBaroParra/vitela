package dev.vitela.pdf.viewer

import androidx.compose.ui.geometry.Offset
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PinchStateTest {
    private fun anchor(fromZoom: Double, toZoom: Double, from: Offset = Offset.Zero, to: Offset = Offset.Zero) =
        PinchAnchor(fromZoom, toZoom, firstItem = 0, firstItemOffset = 0, scrollX = 0, leadX = 0f, from = from, to = to)

    /** A pinch from [laidOut] to [target] whose fingers have lifted, its landing [anchor] handed over. */
    private fun lifted(laidOut: Double, target: Double): PinchState = PinchState().apply {
        assertEquals(laidOut, begin(laidOut, Offset(100f, 100f)), 0.0)
        update(target, Offset(5f, 5f))
        assertEquals(target, release()!!, 0.0)
        land(anchor(laidOut, target))
    }

    /** A live pinch that began at [laidOut] under [origin] and is now asking for [target]. */
    private fun pinching(laidOut: Double, target: Double, origin: Offset = Offset(100f, 200f), shift: Offset = Offset.Zero) =
        PinchState().apply {
            begin(laidOut, origin)
            update(target, shift)
        }

    /** Hands [pinch] the step its own landing points ask for, as the reader does. */
    private fun step(pinch: PinchState, laidOut: Double, now: Long): PinchAnchor {
        val (from, to) = pinch.landingPoints(laidOut)!!
        return anchor(laidOut, pinch.target!!, from, to).also { pinch.step(it, now) }
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
        assertEquals((0.25 / 1.46).toFloat(), pinch.picture(1.46).scale, 1e-6f)
        // Laid out at the asked zoom the picture is the page itself.
        assertEquals(PinchPicture.None, pinch.picture(0.25))
    }

    @Test
    fun liftedPinch_whoseZoomWentElsewhereIsNeverDrawnScaled() {
        // The bug report: 146% laid out, the page drawn at a fifth of it, long after the fingers lifted.
        val pinch = lifted(laidOut = 1.0, target = 0.25)
        assertEquals(PinchPicture.None, pinch.picture(1.46))
    }

    @Test
    fun settledPinch_stopsScaling() {
        val pinch = lifted(laidOut = 1.0, target = 2.0)
        assertFalse(pinch.settle(landed = true, landedAt = null))
        assertNull(pinch.target)
        assertNull(pinch.pending)
        assertEquals(PinchPicture.None, pinch.picture(1.0))
        assertEquals(Offset.Zero, pinch.shift)
    }

    @Test
    fun pinchWithoutALanding_isNotDrawnScaledOnceTheFingersLift() {
        val pinch = pinching(laidOut = 1.0, target = 3.0)
        assertEquals(3f, pinch.picture(1.0).scale, 0f)
        pinch.release()
        assertEquals(PinchPicture.None, pinch.picture(1.0))
    }

    @Test
    fun abandonedPinch_leavesNothingScaled() {
        // The gesture was torn down with the fingers still on the glass: no lift, nothing to commit.
        val pinch = pinching(laidOut = 1.0, target = 0.3, shift = Offset(4f, 4f))
        pinch.abandon()
        assertNull(pinch.target)
        assertEquals(PinchPicture.None, pinch.picture(1.0))
    }

    @Test
    fun pinchStartedBeforeTheLastOneLanded_startsFromTheZoomOnScreen() {
        val pinch = lifted(laidOut = 1.0, target = 2.0)
        // The list still shows 1.0 laid out, scaled to look like 2.0: the new pinch continues from 2.0.
        assertEquals(2.0, pinch.begin(1.0, Offset(10f, 10f)), 0.0)
    }

    @Test
    fun pinchStartedBeforeTheLastOneLanded_keepsThePictureWhereItWas() {
        val pinch = lifted(laidOut = 1.0, target = 2.0)
        val before = pinch.picture(1.0)
        pinch.begin(1.0, Offset(300f, 40f))
        val after = pinch.picture(1.0)
        for (point in listOf(Offset(0f, 0f), Offset(300f, 40f), Offset(720f, 1500f))) {
            assertEquals(before.map(point).x, after.map(point).x, 1e-3f)
            assertEquals(before.map(point).y, after.map(point).y, 1e-3f)
        }
    }

    @Test
    fun pinchAfterALiftWithNoLanding_startsFromTheLaidOutZoom() {
        val pinch = pinching(laidOut = 1.0, target = 3.0)
        pinch.release()
        // Nothing is waiting to land, so the 3.0 left behind is not what the list shows.
        assertEquals(1.0, pinch.begin(1.0, Offset(10f, 10f)), 0.0)
    }

    @Test
    fun landingUnderALivePinch_keepsThatPinchsPicture() {
        val pinch = lifted(laidOut = 1.0, target = 2.0)
        pinch.begin(1.0, Offset(10f, 10f))
        pinch.update(3.0, Offset.Zero)
        pinch.settle(landed = true, landedAt = null)
        assertNull(pinch.pending)
        assertEquals(1.5f, pinch.picture(2.0).scale, 1e-6f)
    }

    @Test
    fun picture_scalesAboutTheFingersAndFollowsThem() {
        val picture = pinching(laidOut = 1.0, target = 2.0, origin = Offset(100f, 200f), shift = Offset(30f, -10f)).picture(1.0)
        assertEquals(2f, picture.scale, 0f)
        // The content first under the fingers is where they moved to.
        assertEquals(Offset(130f, 190f), picture.map(Offset(100f, 200f)))
        assertEquals(Offset(100f, 200f), picture.unmap(Offset(130f, 190f)))
    }

    @Test
    fun step_isDueOnlyOnceAPinchOutShrinksThePicturePastTheRatio() {
        val laidOut = 2.0
        assertFalse(pinching(laidOut, target = laidOut * 0.9).stepDue(laidOut, nowMs = 0))
        assertTrue(pinching(laidOut, target = laidOut * 0.8).stepDue(laidOut, nowMs = 0))
    }

    @Test
    fun step_isNeverDueWhilePinchingIn() {
        assertFalse(pinching(laidOut = 1.0, target = 3.0).stepDue(1.0, nowMs = 0))
    }

    @Test
    fun step_isNotDueOnceTheFingersLifted() {
        val pinch = pinching(laidOut = 2.0, target = 1.0)
        pinch.release()
        assertFalse(pinch.stepDue(2.0, nowMs = 0))
    }

    @Test
    fun step_waitsForThePreviousOneToLandAndForTheInterval() {
        val pinch = pinching(laidOut = 2.0, target = 1.5)
        step(pinch, laidOut = 2.0, now = 1_000)
        pinch.update(1.0, Offset.Zero)
        // The 1.5 step is not laid out yet: a second one would be anchored to a layout nobody will see.
        assertFalse(pinch.stepDue(2.0, nowMs = 2_000))
        assertFalse(pinch.stepDue(1.5, nowMs = 2_000))
        pinch.settle(landed = true, landedAt = null)
        assertFalse(pinch.stepDue(1.5, nowMs = 1_000 + PINCH_STEP_INTERVAL_MS - 1))
        assertTrue(pinch.stepDue(1.5, nowMs = 1_000 + PINCH_STEP_INTERVAL_MS))
    }

    @Test
    fun step_isNeverDueOverALayoutThePinchKnowsNothingAbout() {
        // The zoom changed under the fingers some other way: there is no picture to anchor a step to.
        assertFalse(pinching(laidOut = 2.0, target = 1.0).stepDue(1.7, nowMs = 0))
    }

    @Test
    fun step_isAWaitingLandingNotADrop() {
        val pinch = pinching(laidOut = 2.0, target = 1.5)
        val anchor = step(pinch, laidOut = 2.0, now = 0)
        assertEquals(PinchLanding.Wait, anchor.landingAt(2.0))
        assertEquals(PinchLanding.Land, anchor.landingAt(1.5))
    }

    @Test
    fun step_keepsThePictureUnmovedBeforeAndAfterItsZoomIsLaidOut() {
        val pinch = pinching(laidOut = 2.0, target = 1.5, origin = Offset(400f, 600f), shift = Offset(-20f, 35f))
        val before = pinch.picture(2.0)
        step(pinch, laidOut = 2.0, now = 0)
        // The step's zoom is not laid out yet: the old picture stays as it was.
        assertEquals(before, pinch.picture(2.0))
        // Laid out at 1.5 the list itself already looks like 1.5: nothing left to scale or move.
        assertEquals(1f, pinch.picture(1.5).scale, 0f)
        assertEquals(Offset.Zero, pinch.picture(1.5).translation)
        pinch.settle(landed = true, landedAt = null)
        assertEquals(1f, pinch.picture(1.5).scale, 0f)
    }

    @Test
    fun step_keepsTheContentUnderTheFingersAcrossTheRestOfThePinch() {
        val origin = Offset(400f, 600f)
        val stepped = pinching(laidOut = 2.0, target = 1.5, origin = origin, shift = Offset(-20f, 35f))
        // The content under the fingers is where the step lands it: at origin + shift.
        val (from, to) = stepped.landingPoints(2.0)!!
        assertEquals(origin, from)
        assertEquals(Offset(380f, 635f), to)
        step(stepped, laidOut = 2.0, now = 0)
        stepped.settle(landed = true, landedAt = null)
        // The fingers carry on to 1.0 and move again. Without the step the
        // 2.0 layout would be scaled by 0.5; with it, the 1.5 layout by 2/3.
        // Either way that content ends up where the fingers are.
        stepped.update(1.0, Offset(10f, 50f))
        assertEquals((1.0 / 1.5).toFloat(), stepped.picture(1.5).scale, 1e-6f)
        val fingers = stepped.picture(1.5).map(to)
        assertEquals(origin.x + 10f, fingers.x, 1e-3f)
        assertEquals(origin.y + 50f, fingers.y, 1e-3f)
        // And the release lands that content there, from the 1.5 layout.
        assertEquals(to to Offset(410f, 650f), stepped.landingPoints(1.5))
    }

    @Test
    fun step_thatTheListCouldNotScrollToIsMadeUpByThePicture() {
        val pinch = pinching(laidOut = 2.0, target = 1.5, origin = Offset(400f, 600f))
        step(pinch, laidOut = 2.0, now = 0)
        // The list stopped at its end: the content under the fingers sits 80 px lower than asked.
        pinch.settle(landed = true, landedAt = Offset(400f, 680f))
        val picture = pinch.picture(1.5)
        assertEquals(Offset(400f, 600f), picture.map(Offset(400f, 680f)))
        assertEquals(Offset(400f, 680f) to Offset(400f, 600f), pinch.landingPoints(1.5))
    }

    @Test
    fun release_whileAStepIsLanding_waitsForIt() {
        val pinch = pinching(laidOut = 2.0, target = 1.5)
        step(pinch, laidOut = 2.0, now = 0)
        pinch.update(1.2, Offset.Zero)
        assertNull(pinch.release())
        assertTrue(pinch.releaseDeferred)
        // Fingers up, nothing committed yet: the picture still stands in for the list.
        assertEquals((1.2 / 2.0).toFloat(), pinch.picture(2.0).scale, 1e-6f)
        // The step landed: the release is due now, from the step's layout.
        assertTrue(pinch.settle(landed = true, landedAt = null))
        assertFalse(pinch.releaseDeferred)
        assertEquals(1.2, pinch.target!!, 0.0)
        val (from, to) = pinch.landingPoints(1.5)!!
        pinch.land(anchor(1.5, 1.2, from, to))
        assertEquals((1.2 / 1.5).toFloat(), pinch.picture(1.5).scale, 1e-6f)
    }

    @Test
    fun release_whoseStepCouldNeverLand_leavesNothingScaled() {
        val pinch = pinching(laidOut = 2.0, target = 1.5)
        step(pinch, laidOut = 2.0, now = 0)
        pinch.release()
        assertFalse(pinch.settle(landed = false, landedAt = null))
        assertFalse(pinch.releaseDeferred)
        assertNull(pinch.target)
        assertEquals(PinchPicture.None, pinch.picture(1.7))
    }

    @Test
    fun droppedStepUnderTheFingers_drawsTheListAsLaidOut() {
        val pinch = pinching(laidOut = 2.0, target = 1.5)
        step(pinch, laidOut = 2.0, now = 0)
        pinch.settle(landed = false, landedAt = null)
        assertEquals(PinchPicture.None, pinch.picture(1.7))
        // With no layout it knows, there is nothing to land from either.
        assertNull(pinch.landingPoints(1.7))
    }

    @Test
    fun landedPoint_isWhereTheAnchorAskedWhenNothingClamped() {
        val anchor = PinchAnchor(
            fromZoom = 2.0, toZoom = 1.0, firstItem = 3, firstItemOffset = 400, scrollX = 600, leadX = 0f,
            from = Offset(300f, 500f), to = Offset(320f, 520f),
        )
        val scrollX = anchoredScroll(anchor.scrollX, anchor.from.x, anchor.to.x, 0.5, anchor.leadX)
        val offset = anchoredScroll(anchor.firstItemOffset, anchor.from.y, anchor.to.y, 0.5)
        val landed = landedPoint(anchor, itemTop = -offset, scrollX = scrollX, lead = 0f)
        assertEquals(320f, landed.x, 1f)
        assertEquals(520f, landed.y, 1f)
    }

    @Test
    fun landedPoint_measuresTheClampedScrollAndTheCentringMargin() {
        val anchor = PinchAnchor(
            fromZoom = 1.0, toZoom = 0.5, firstItem = 0, firstItemOffset = 100, scrollX = 0, leadX = 0f,
            from = Offset(400f, 300f), to = Offset(400f, 300f),
        )
        // Half as wide, the column is centred 250 px in and cannot scroll; the list stopped at its top.
        val landed = landedPoint(anchor, itemTop = 0, scrollX = 0, lead = 250f)
        assertEquals(450f, landed.x, 1e-3f)
        assertEquals(200f, landed.y, 1e-3f)
    }

    @Test
    fun landedPoint_ofAnItemNoLongerOnScreen_trustsTheAnchor() {
        val anchor = anchor(fromZoom = 2.0, toZoom = 1.0, to = Offset(10f, 90f))
        assertEquals(90f, landedPoint(anchor, itemTop = null, scrollX = 0, lead = 0f).y, 0f)
    }
}
