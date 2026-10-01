package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.BlockSource
import dev.vitela.pdf.core.DocumentBlock
import dev.vitela.pdf.core.PageEdit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The Documents view's decisions that need no core: what a card says, and what a block edit asks of the core. */
class OrganizeDocumentsTest {
    private val base1 = DocumentBlock(BlockSource.Base, part = 1, start = 0, count = 2)
    private val added = DocumentBlock(BlockSource.Imported(7), part = null, start = 2, count = 3)
    private val base2 = DocumentBlock(BlockSource.Base, part = 2, start = 5, count = 1)
    private val blocks = listOf(base1, added, base2)

    @Test
    fun aCardIsNamedAfterItsSourceAndItsPart() {
        val names = mapOf(7L to "added.pdf")

        assertEquals("report.pdf — Part 1", blockTitle(base1, "report.pdf", names))
        assertEquals("added.pdf", blockTitle(added, "report.pdf", names))
        assertEquals("Blank pages", blockTitle(DocumentBlock(BlockSource.Blank, null, 0, 1), "report.pdf", names))
        assertEquals("Imported PDF", blockTitle(DocumentBlock(BlockSource.Imported(9), null, 0, 1), "report.pdf", names))
    }

    @Test
    fun aCardSaysHowBigTheBlockIsAndWhereItSits() {
        assertEquals("3 pages · 3–5", blockMeta(added))
        assertEquals("1 page · 6", blockMeta(base2))
    }

    @Test
    fun movingABlockEarlierPutsItWhereThePreviousBlockStarts() {
        assertEquals(PageEdit.Move(from = 2, to = 0, count = 3), blockMove(blocks, added, -1))
    }

    @Test
    fun movingABlockLaterPutsItAfterTheNextBlock() {
        assertEquals(PageEdit.Move(from = 2, to = 3, count = 3), blockMove(blocks, added, 1))
        assertEquals(PageEdit.Move(from = 0, to = 3, count = 2), blockMove(blocks, base1, 1))
    }

    @Test
    fun aBlockCannotMovePastEitherEnd() {
        assertNull(blockMove(blocks, base1, -1))
        assertNull(blockMove(blocks, base2, 1))
    }

    @Test
    fun aBlockThatIsNoLongerThereCannotMove() {
        assertNull(blockMove(blocks, added.copy(start = 1), 1))
    }

    @Test
    fun aBlockMoveCarriesEveryPictureWithIt() {
        val thumbnails = (0 until 6).associateWith { "p$it" }

        val moved = remapAfterEdit(thumbnails, PageEdit.Move(from = 2, to = 0, count = 3))

        assertEquals(listOf("p2", "p3", "p4", "p0", "p1", "p5"), (0 until 6).map { moved[it] })
    }

    @Test
    fun aBlockMovedLaterShiftsTheSkippedPagesBack() {
        val thumbnails = (0 until 6).associateWith { "p$it" }

        val moved = remapAfterEdit(thumbnails, PageEdit.Move(from = 0, to = 3, count = 2))

        assertEquals(listOf("p2", "p3", "p4", "p0", "p1", "p5"), (0 until 6).map { moved[it] })
    }

    @Test
    fun turningABlockForgetsOnlyItsPictures() {
        val thumbnails = (0 until 4).associateWith { "p$it" }

        assertEquals(mapOf(0 to "p0", 3 to "p3"), remapAfterEdit(thumbnails, PageEdit.Rotate(1, 90, count = 2)))
    }

    @Test
    fun deletingABlockClosesTheGapItLeaves() {
        val thumbnails = (0 until 5).associateWith { "p$it" }

        assertEquals(mapOf(0 to "p0", 1 to "p3", 2 to "p4"), remapAfterEdit(thumbnails, PageEdit.Remove(1, count = 2)))
    }

    @Test
    fun aBlockHoldingEveryPageCannotBeDeleted() {
        assertEquals(ORGANIZE_LAST_PAGE, organizeRefusal(3, PageEdit.Remove(0, count = 3)))
        assertNull(organizeRefusal(3, PageEdit.Remove(0, count = 2)))
    }

    @Test
    fun aRunPastTheEndIsRefusedBeforeTheCore() {
        assertEquals(ORGANIZE_NO_SUCH_PAGE, organizeRefusal(3, PageEdit.Remove(2, count = 2)))
        assertEquals(ORGANIZE_NO_SUCH_PAGE, organizeRefusal(3, PageEdit.Rotate(2, 90, count = 2)))
        assertEquals(ORGANIZE_NO_SUCH_PAGE, organizeRefusal(4, PageEdit.Move(from = 0, to = 3, count = 2)))
        assertEquals(ORGANIZE_NO_SUCH_PAGE, organizeRefusal(4, PageEdit.Move(from = 0, to = 2, count = 0)))
    }

    @Test
    fun theStatusCountsTheBlocksPages() {
        assertEquals("3 pages moved. Changes are pending save.", organizeStatus(PageEdit.Move(2, 0, count = 3)))
        assertEquals("2 pages rotated. Changes are pending save.", organizeStatus(PageEdit.Rotate(0, 90, count = 2)))
        assertEquals("2 pages deleted. Changes are pending save.", organizeStatus(PageEdit.Remove(0, count = 2)))
        assertEquals("Page moved. Changes are pending save.", organizeStatus(PageEdit.Move(2, 0)))
    }
}
