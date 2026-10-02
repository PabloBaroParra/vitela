package dev.vitela.pdf.viewer

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** A typed page number names a page of the document, or nothing: it is never clamped or rounded. */
class PageNumberInputTest {
    @Test
    fun aPageNumberIsOneBasedAndBecomesItsIndex() {
        assertEquals(0, typedPageIndex("1", pageCount = 5))
        assertEquals(4, typedPageIndex("5", pageCount = 5))
        assertEquals(2, typedPageIndex(" 3 ", pageCount = 5))
    }

    @Test
    fun aNumberOutsideTheDocumentIsRefusedNotClamped() {
        assertNull(typedPageIndex("0", pageCount = 5))
        assertNull(typedPageIndex("6", pageCount = 5))
        assertNull(typedPageIndex("-1", pageCount = 5))
        assertNull(typedPageIndex("99999999999", pageCount = 5))
    }

    @Test
    fun onlyWholeNumbersArePageNumbers() {
        assertNull(typedPageIndex("", pageCount = 5))
        assertNull(typedPageIndex("2.5", pageCount = 5))
        assertNull(typedPageIndex("2.0", pageCount = 5))
        assertNull(typedPageIndex("two", pageCount = 5))
    }

    @Test
    fun anEmptyDocumentHasNoPageNumbers() {
        assertNull(typedPageIndex("1", pageCount = 0))
    }
}
