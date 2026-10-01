package dev.vitela.pdf.print

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PrintPagesTest {
    @Test
    fun allPagesIsTheWholeDocument() {
        // PageRange.ALL_PAGES is 0..Int.MAX_VALUE.
        assertEquals(listOf(0, 1, 2), pagesToPrint(listOf(0..Int.MAX_VALUE), pageCount = 3))
    }

    @Test
    fun aRangePastTheEndIsClamped() {
        assertEquals(listOf(1, 2), pagesToPrint(listOf(1..9), pageCount = 3))
    }

    @Test
    fun aRangeEntirelyPastTheEndPrintsNothing() {
        assertEquals(emptyList<Int>(), pagesToPrint(listOf(5..8), pageCount = 3))
    }

    @Test
    fun aNegativeStartIsClampedToTheFirstPage() {
        assertEquals(listOf(0, 1), pagesToPrint(listOf(-4..1), pageCount = 3))
    }

    @Test
    fun separateRangesKeepTheirPagesInOrder() {
        assertEquals(listOf(0, 2, 3), pagesToPrint(listOf(0..0, 2..3), pageCount = 4))
    }

    @Test
    fun overlappingAndUnsortedRangesNeverRepeatOrReorderAPage() {
        assertEquals(listOf(0, 1, 2, 3), pagesToPrint(listOf(2..3, 0..2), pageCount = 4))
    }

    @Test
    fun anEmptyDocumentPrintsNothing() {
        assertEquals(emptyList<Int>(), pagesToPrint(listOf(0..Int.MAX_VALUE), pageCount = 0))
    }

    @Test
    fun noRangesPrintsNothing() {
        assertEquals(emptyList<Int>(), pagesToPrint(emptyList(), pageCount = 3))
    }

    @Test
    fun anEmptyRangeIsIgnored() {
        assertEquals(listOf(2), pagesToPrint(listOf(IntRange.EMPTY, 2..2), pageCount = 3))
    }

    @Test
    fun consecutivePagesMergeIntoOneWrittenRange() {
        assertEquals(listOf(0..2, 4..4, 6..7), writtenRanges(listOf(0, 1, 2, 4, 6, 7)))
    }

    @Test
    fun nothingWrittenReportsNoRanges() {
        assertEquals(emptyList<IntRange>(), writtenRanges(emptyList()))
    }

    @Test
    fun aPageTallerThanTheBoxIsLimitedByHeightAndCentredAcross() {
        // 100x200 into 300x200: scale 1, 100 spare columns on each side.
        assertEquals(Placement(1f, 100f, 0f), fitCentered(100f, 200f, 300f, 200f))
    }

    @Test
    fun aPageWiderThanTheBoxIsLimitedByWidthAndCentredDown() {
        // 400x100 into 200x200: scale 0.5, the 50-tall bitmap leaves 75 above and below.
        assertEquals(Placement(0.5f, 0f, 75f), fitCentered(400f, 100f, 200f, 200f))
    }

    @Test
    fun aPageThatAlreadyFitsExactlyIsUntouched() {
        assertEquals(Placement(1f, 0f, 0f), fitCentered(612f, 792f, 612f, 792f))
    }

    @Test
    fun aSmallerPageIsScaledUpToFill() {
        assertEquals(Placement(2f, 0f, 0f), fitCentered(100f, 100f, 200f, 200f))
    }

    @Test
    fun degenerateSizesHaveNoPlacement() {
        assertNull(fitCentered(0f, 100f, 100f, 100f))
        assertNull(fitCentered(100f, -1f, 100f, 100f))
        assertNull(fitCentered(100f, 100f, 0f, 100f))
        assertNull(fitCentered(100f, 100f, 100f, Float.NaN))
        assertNull(fitCentered(Float.POSITIVE_INFINITY, 100f, 100f, 100f))
    }
}
