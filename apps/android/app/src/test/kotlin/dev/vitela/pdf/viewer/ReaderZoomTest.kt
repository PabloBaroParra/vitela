package dev.vitela.pdf.viewer

import org.junit.Assert.assertEquals
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
}
