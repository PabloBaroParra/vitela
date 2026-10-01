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
        assertEquals(700, anchoredScroll(scroll = 300, focus = 100f, ratio = 2.0))
        // Halved, it is at 200, so the viewport starts at 100.
        assertEquals(100, anchoredScroll(scroll = 300, focus = 100f, ratio = 0.5))
    }

    @Test
    fun anchoredScroll_neverAsksForANegativeOffset() {
        assertEquals(0, anchoredScroll(scroll = 0, focus = 400f, ratio = 0.5))
    }
}
