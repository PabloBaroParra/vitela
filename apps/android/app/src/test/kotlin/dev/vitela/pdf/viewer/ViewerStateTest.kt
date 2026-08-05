package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.SaveSnapshot
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ViewerStateTest {
    @Test
    fun boundedPageIndex_staysWithinDocument() {
        assertEquals(0, boundedPageIndex(-4, 3))
        assertEquals(2, boundedPageIndex(9, 3))
    }

    @Test
    fun nextSearchIndex_wrapsInBothDirections() {
        assertEquals(0, nextSearchIndex(2, 3, 1))
        assertEquals(2, nextSearchIndex(0, 3, -1))
    }

    @Test
    fun zoomSteps_followTheDiscreteLadderAndClampAtBothEnds() {
        assertEquals(1.25, zoomIn(1.0), 0.0)
        assertEquals(0.75, zoomOut(1.0), 0.0)
        assertEquals(1.5, zoomIn(1.3), 0.0)
        assertEquals(1.25, zoomOut(1.3), 0.0)
        assertEquals(MAX_ZOOM_FACTOR, zoomIn(MAX_ZOOM_FACTOR), 0.0)
        assertEquals(MIN_ZOOM_FACTOR, zoomOut(MIN_ZOOM_FACTOR), 0.0)
    }

    @Test
    fun zoomClamp_rejectsOutOfRangeAndNonFiniteFactors() {
        assertEquals(MIN_ZOOM_FACTOR, clampZoomFactor(0.001), 0.0)
        assertEquals(MAX_ZOOM_FACTOR, clampZoomFactor(99.0), 0.0)
        assertEquals(DEFAULT_ZOOM_FACTOR, clampZoomFactor(Double.NaN), 0.0)
    }

    @Test
    fun saveSnapshot_matchesOnlyTheDocumentRevisionThatCreatedIt() {
        val snapshot = SaveSnapshot(byteArrayOf(1), documentId = 7, revision = 3)

        assertTrue(ViewerState(documentId = 7, revision = 3).matches(snapshot))
        assertFalse(ViewerState(documentId = 8, revision = 3).matches(snapshot))
        assertFalse(ViewerState(documentId = 7, revision = 4).matches(snapshot))
    }
}
