package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PageRotation
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.SaveSnapshot
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
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
    fun fitPage_fitsTheDrawnPageHeightAndNeverPassesFitToWidth() {
        // A portrait page twice as tall as wide: 1000 px wide is 2000 px tall.
        assertEquals(0.6, fitPageZoomFactor(PageSize(100.0, 200.0), 1000, 1200)!!, 1e-9)
        // A quarter-turned page is drawn landscape, so it already fits at width.
        assertEquals(DEFAULT_ZOOM_FACTOR, fitPageZoomFactor(PageSize(200.0, 100.0, PageRotation.Clockwise90), 1000, 1200)!!, 0.0)
        // A sliver of a viewport still clamps to the smallest rung.
        assertEquals(MIN_ZOOM_FACTOR, fitPageZoomFactor(PageSize(100.0, 200.0), 1000, 10)!!, 0.0)
    }

    @Test
    fun fitPage_hasNoAnswerWithoutAPageOrAViewport() {
        assertNull(fitPageZoomFactor(null, 1000, 1200))
        assertNull(fitPageZoomFactor(PageSize(100.0, 0.0), 1000, 1200))
        assertNull(fitPageZoomFactor(PageSize(100.0, 200.0), 1000, 0))
        assertNull(fitPageZoomFactor(PageSize(100.0, 200.0), 0, 1200))
    }

    @Test
    fun saveSnapshot_matchesOnlyTheDocumentRevisionThatCreatedIt() {
        val snapshot = SaveSnapshot(byteArrayOf(1), documentId = 7, revision = 3)

        assertTrue(ViewerState(documentId = 7, revision = 3).matches(snapshot))
        assertFalse(ViewerState(documentId = 8, revision = 3).matches(snapshot))
        assertFalse(ViewerState(documentId = 7, revision = 4).matches(snapshot))
    }
}
