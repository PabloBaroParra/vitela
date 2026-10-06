package dev.vitela.pdf.viewer

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** First and Last page jump to either end, and are unavailable once the reader is already there. */
class PageBoundaryTest {
    @Test
    fun firstPageJumpsToTheStartFromAnyLaterPage() {
        assertEquals(0, firstPageTarget(currentPage = 3, pageCount = 5))
        assertEquals(0, firstPageTarget(currentPage = 1, pageCount = 5))
    }

    @Test
    fun lastPageJumpsToTheEndFromAnyEarlierPage() {
        assertEquals(4, lastPageTarget(currentPage = 0, pageCount = 5))
        assertEquals(4, lastPageTarget(currentPage = 3, pageCount = 5))
    }

    @Test
    fun aJumpToThePageAlreadyShownIsUnavailable() {
        assertNull(firstPageTarget(currentPage = 0, pageCount = 5))
        assertNull(lastPageTarget(currentPage = 4, pageCount = 5))
    }

    @Test
    fun aSinglePageDocumentHasNowhereToJump() {
        assertNull(firstPageTarget(currentPage = 0, pageCount = 1))
        assertNull(lastPageTarget(currentPage = 0, pageCount = 1))
    }

    @Test
    fun anEmptyDocumentHasNoBoundaries() {
        assertNull(firstPageTarget(currentPage = 0, pageCount = 0))
        assertNull(lastPageTarget(currentPage = 0, pageCount = 0))
    }

    @Test
    fun aStaleCurrentPageIsBoundedBeforeDeciding() {
        // The page counter can lag a page-structure change for a frame.
        assertNull(lastPageTarget(currentPage = 9, pageCount = 5))
        assertEquals(0, firstPageTarget(currentPage = 9, pageCount = 5))
    }
}

/** Previous and Next page step one page and stop at either end rather than wrap (parity with Windows). */
class PageStepTest {
    @Test
    fun previousStepsBackOnePage() {
        assertEquals(2, previousPageTarget(currentPage = 3, pageCount = 5))
        assertEquals(0, previousPageTarget(currentPage = 1, pageCount = 5))
    }

    @Test
    fun nextStepsForwardOnePage() {
        assertEquals(1, nextPageTarget(currentPage = 0, pageCount = 5))
        assertEquals(4, nextPageTarget(currentPage = 3, pageCount = 5))
    }

    @Test
    fun neitherWrapsPastAnEnd() {
        assertNull(previousPageTarget(currentPage = 0, pageCount = 5))
        assertNull(nextPageTarget(currentPage = 4, pageCount = 5))
    }

    @Test
    fun aSinglePageDocumentHasNowhereToStep() {
        assertNull(previousPageTarget(currentPage = 0, pageCount = 1))
        assertNull(nextPageTarget(currentPage = 0, pageCount = 1))
    }

    @Test
    fun anEmptyDocumentHasNowhereToStep() {
        assertNull(previousPageTarget(currentPage = 0, pageCount = 0))
        assertNull(nextPageTarget(currentPage = 0, pageCount = 0))
    }

    @Test
    fun aStaleCurrentPageIsBoundedBeforeStepping() {
        // The page counter can lag a page removal: step from the last real page.
        assertNull(nextPageTarget(currentPage = 9, pageCount = 5))
        assertEquals(3, previousPageTarget(currentPage = 9, pageCount = 5))
    }
}
