package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PageEdit
import dev.vitela.pdf.core.PageSize
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The pure half of Organize: what an edit means for positions, thumbnails and the refusal wording. */
class PageOrganizeTest {
    // A card travels with its page: a thumbnail keyed by position is remapped, not re-rendered.
    private val cards = mapOf(0 to "a", 1 to "b", 2 to "c", 3 to "d")

    @Test
    fun movingForwardShiftsTheSkippedPagesBack() {
        // b goes from 1 to 3: c and d each step one position toward the front.
        assertEquals(mapOf(0 to "a", 1 to "c", 2 to "d", 3 to "b"), remapAfterEdit(cards, PageEdit.Move(1, 3)))
    }

    @Test
    fun movingBackwardShiftsTheSkippedPagesForward() {
        assertEquals(mapOf(0 to "d", 1 to "a", 2 to "b", 3 to "c"), remapAfterEdit(cards, PageEdit.Move(3, 0)))
    }

    @Test
    fun aMoveKeepsOnlyTheThumbnailsItHad() {
        assertEquals(mapOf(0 to "a", 2 to "b"), remapAfterEdit(mapOf(0 to "a", 1 to "b"), PageEdit.Move(1, 2)))
    }

    @Test
    fun turningAPageForgetsOnlyItsThumbnail() {
        assertEquals(mapOf(0 to "a", 2 to "c", 3 to "d"), remapAfterEdit(cards, PageEdit.Rotate(1, 90)))
    }

    @Test
    fun deletingAPageClosesTheGapItLeaves() {
        assertEquals(mapOf(0 to "a", 1 to "c", 2 to "d"), remapAfterEdit(cards, PageEdit.Remove(1)))
    }

    @Test
    fun deletingTheLastPageDropsItsThumbnailAndShiftsNothing() {
        assertEquals(mapOf(0 to "a", 1 to "b", 2 to "c"), remapAfterEdit(cards, PageEdit.Remove(3)))
    }

    @Test
    fun insertingABlankPageShiftsEveryCardFromThatPositionOn() {
        // The blank card has no picture yet; the pages it pushed back keep theirs.
        assertEquals(mapOf(0 to "a", 2 to "b", 3 to "c", 4 to "d"), remapAfterEdit(cards, PageEdit.InsertBlank(1)))
    }

    @Test
    fun appendingABlankPageMovesNoCard() {
        assertEquals(cards, remapAfterEdit(cards, PageEdit.InsertBlank(4, landscape = true)))
    }

    @Test
    fun aBlankPageGoesBeforeAnyPageOrAfterTheLast() {
        assertNull(organizeRefusal(pageCount = 3, edit = PageEdit.InsertBlank(0)))
        assertNull(organizeRefusal(pageCount = 3, edit = PageEdit.InsertBlank(3)))
        assertEquals(ORGANIZE_NO_SUCH_PAGE, organizeRefusal(pageCount = 3, edit = PageEdit.InsertBlank(4)))
        assertEquals(ORGANIZE_NO_SUCH_PAGE, organizeRefusal(pageCount = 3, edit = PageEdit.InsertBlank(-1)))
    }

    @Test
    fun aNeighbourMoveTargetsTheAdjacentPosition() {
        // "to" is where the page sits afterwards, so one step later from 1 is 2.
        assertEquals(PageEdit.Move(1, 2), neighbourMove(index = 1, delta = 1, pageCount = 4))
        assertEquals(PageEdit.Move(1, 0), neighbourMove(index = 1, delta = -1, pageCount = 4))
    }

    @Test
    fun aNeighbourMovePastEitherEndIsNothing() {
        assertNull(neighbourMove(index = 0, delta = -1, pageCount = 4))
        assertNull(neighbourMove(index = 3, delta = 1, pageCount = 4))
        assertNull(neighbourMove(index = 9, delta = -1, pageCount = 4))
    }

    @Test
    fun theLastPageCannotBeDeleted() {
        assertEquals(ORGANIZE_LAST_PAGE, organizeRefusal(pageCount = 1, edit = PageEdit.Remove(0)))
        assertNull(organizeRefusal(pageCount = 2, edit = PageEdit.Remove(0)))
    }

    @Test
    fun aOnePageDocumentCanStillBeTurned() {
        assertNull(organizeRefusal(pageCount = 1, edit = PageEdit.Rotate(0, 90)))
    }

    @Test
    fun aPositionOutsideTheDocumentIsRefusedBeforeTheCore() {
        assertEquals(ORGANIZE_NO_SUCH_PAGE, organizeRefusal(3, PageEdit.Rotate(3, 90)))
        assertEquals(ORGANIZE_NO_SUCH_PAGE, organizeRefusal(3, PageEdit.Remove(-1)))
        assertEquals(ORGANIZE_NO_SUCH_PAGE, organizeRefusal(3, PageEdit.Move(0, 3)))
    }

    @Test
    fun onlyAQuarterTurnIsOffered() {
        assertEquals(ORGANIZE_QUARTER_TURNS, organizeRefusal(3, PageEdit.Rotate(0, 45)))
        assertNull(organizeRefusal(3, PageEdit.Rotate(0, -90)))
    }

    @Test
    fun aMoveOntoItselfIsNotAnEdit() {
        assertEquals(ORGANIZE_NOTHING_TO_DO, organizeRefusal(3, PageEdit.Move(1, 1)))
    }

    @Test
    fun theStatusNamesTheEditAndSaysItIsPending() {
        assertEquals("Page moved. Changes are pending save.", organizeStatus(PageEdit.Move(0, 1)))
        assertEquals("Page rotated. Changes are pending save.", organizeStatus(PageEdit.Rotate(0, 90)))
        assertEquals("Page deleted. Changes are pending save.", organizeStatus(PageEdit.Remove(0)))
        assertEquals("Blank page added. Changes are pending save.", organizeStatus(PageEdit.InsertBlank(0)))
    }

    @Test
    fun aThumbnailFitsItsLongerSideIntoTheBox() {
        // 792 pt long side into 220 px: 220 * 72 / 792 = 20 dpi.
        assertEquals(20, thumbnailDpi(PageSize(612.0, 792.0), boxPx = 220))
        // A landscape page is fitted by its width, which is now the longer side.
        assertEquals(20, thumbnailDpi(PageSize(792.0, 612.0), boxPx = 220))
    }

    @Test
    fun aDegeneratePageStillGetsAUsableThumbnailDensity() {
        assertEquals(FALLBACK_THUMBNAIL_DPI, thumbnailDpi(PageSize(0.0, 0.0), boxPx = 220))
        assertEquals(FALLBACK_THUMBNAIL_DPI, thumbnailDpi(null, boxPx = 220))
        assertEquals(MAX_RENDER_DPI, thumbnailDpi(PageSize(1.0, 1.0), boxPx = 220))
    }

    @Test
    fun theGridKeepsOnlyUndoAndRedoOfTheAnnotationToolbar() {
        val controls = AnnotationControls(canCreate = true, canMove = true, canResize = true, canRestyle = true, canGrow = true, canUndo = true, canRedo = false)
        assertEquals(AnnotationControls.disabled.copy(canUndo = true), controls.whileOrganizing())
    }

    @Test
    fun theThumbnailCacheKeepsTheOnesNearestTheFocus() {
        val many = (0 until 10).associateWith { "t$it" }
        val trimmed = trimThumbnails(many, around = 8, limit = 4)
        assertEquals(setOf(6, 7, 8, 9), trimmed.keys)
    }

    @Test
    fun aCacheUnderTheLimitIsLeftAlone() {
        val few = mapOf(0 to "a", 5 to "b")
        assertEquals(few, trimThumbnails(few, around = 0, limit = 4))
    }
}
