package dev.vitela.pdf.viewer

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ReaderLayoutTest {
    @Test fun windowWidthControlsTabletAndSpreadAvailability() {
        assertFalse(readerLayout(599f).sideTools)
        assertTrue(readerLayout(600f).sideTools)
        assertFalse(readerLayout(999f).canShowSpread)
        assertTrue(readerLayout(1000f).canShowSpread)
    }

    @Test fun oddDocumentDoesNotReportOrRenderANonexistentPage() {
        val rows = PageRows(5, 2)
        assertEquals(3, rows.count)
        assertEquals(4, rows.first(2))
        assertEquals(4, rows.last(2))
        assertEquals(2, rows.row(4))
    }

    @Test fun navigationToRightPageKeepsThatPageCurrent() {
        val rows = PageRows(8, 2)
        assertEquals(3, rows.current(rows.row(3), 3))
        assertEquals(4, rows.current(2, 3))
    }

    @Test fun switchingLayoutsKeepsDocumentPageMapping() {
        assertEquals(7, PageRows(10, 1).row(7))
        assertEquals(3, PageRows(10, 2).row(7))
        assertEquals(7, PageRows(10, 1).current(7, 7))
    }

    @Test fun emptyAndSinglePageDocumentsRemainBounded() {
        assertEquals(0, PageRows(0, 2).count)
        val rows = PageRows(1, 2)
        assertEquals(1, rows.count)
        assertEquals(0, rows.last(0))
        assertEquals(0, rows.current(0, 1))
    }
}
