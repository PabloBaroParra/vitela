package dev.vitela.pdf.viewer

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * T-088: the drawn signature becomes a PNG cropped to the ink, so the core's
 * stamp placement sizes the signature and not the empty pad around it.
 */
class DrawnSignatureTest {
    @Test
    fun aPadWithoutALineHasNoSignature() {
        assertFalse(hasSignatureInk(emptyList()))
        assertFalse(hasSignatureInk(listOf(listOf(PadPoint(5f, 5f)))))
        assertNull(signatureFrame(listOf(listOf(PadPoint(5f, 5f))), strokeWidth = 4f))
    }

    @Test
    fun theFrameHugsTheInkPlusHalfAStroke() {
        val strokes = listOf(
            listOf(PadPoint(100f, 50f), PadPoint(300f, 80f)),
            listOf(PadPoint(5f, 5f)), // a tap leaves no line, so it does not widen the frame
            listOf(PadPoint(150f, 120f), PadPoint(200f, 60f)),
        )

        val frame = requireNotNull(signatureFrame(strokes, strokeWidth = 8f))

        assertEquals(96f, frame.left)
        assertEquals(46f, frame.top)
        assertEquals(208f, frame.width)
        assertEquals(78f, frame.height)
        assertEquals(1f, frame.scale)
        assertEquals(208, frame.pixelWidth)
        assertEquals(78, frame.pixelHeight)
    }

    @Test
    fun aLargeSignatureIsScaledDownToTheLongSideCap() {
        val strokes = listOf(listOf(PadPoint(0f, 0f), PadPoint(2396f, 596f)))

        val frame = requireNotNull(signatureFrame(strokes, strokeWidth = 4f, maxSidePx = 1200))

        assertEquals(0.5f, frame.scale)
        assertEquals(1200, frame.pixelWidth)
        assertEquals(300, frame.pixelHeight)
    }

    @Test
    fun aStraightLineStillHasAPixelOfHeight() {
        val frame = requireNotNull(signatureFrame(listOf(listOf(PadPoint(0f, 10f), PadPoint(100f, 10f))), strokeWidth = 0f))

        assertTrue(frame.pixelHeight >= 1)
    }
}
