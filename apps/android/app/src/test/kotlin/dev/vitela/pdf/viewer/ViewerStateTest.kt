package dev.vitela.pdf.viewer

import org.junit.Assert.assertEquals
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
}
