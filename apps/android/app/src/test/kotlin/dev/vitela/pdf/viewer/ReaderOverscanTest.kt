package dev.vitela.pdf.viewer

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ReaderOverscanTest {
    @Test
    fun coveredPictureScale_isOneWithoutOverscan() {
        // Nothing laid out past the viewport: any shrink at all uncovers an empty band.
        assertEquals(1f, coveredPictureScale(0f, centred = false), 0f)
        assertEquals(1f, coveredPictureScale(0f, centred = true), 0f)
    }

    @Test
    fun coveredPictureScale_countsOneSideForAPinchAtTheEdge() {
        // Fingers at the very edge push everything the shrink reveals to the far side.
        assertEquals(2f / 3f, coveredPictureScale(0.5f, centred = false), 1e-6f)
    }

    @Test
    fun coveredPictureScale_countsBothSidesForACentredPinch() {
        assertEquals(0.5f, coveredPictureScale(0.5f, centred = true), 1e-6f)
    }

    @Test
    fun readerOverscan_coversAPictureAtHalfSizeUnderCentredFingers() {
        assertTrue(coveredPictureScale(READER_OVERSCAN, centred = true) <= 0.5f)
    }

    @Test
    fun pinchStep_isAskedWhileTheOverscanStillCoversThePicture() {
        // A step lands a frame or two after it is asked; until then the picture keeps shrinking over the old layout.
        val fastestPinchOverALanding = 0.78
        assertTrue(PINCH_STEP_RATIO * fastestPinchOverALanding > coveredPictureScale(READER_OVERSCAN, centred = false))
    }
}
