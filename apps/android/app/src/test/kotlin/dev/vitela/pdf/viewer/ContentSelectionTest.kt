package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentFontKind
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.ContentPath
import dev.vitela.pdf.core.ContentSelection
import dev.vitela.pdf.core.ContentTextBlock
import dev.vitela.pdf.core.ContentTextRun
import dev.vitela.pdf.core.PathPaint
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The pure half of multi-selection (B25 T-208): the marquee's hit test, toggling, the union box. */
class ContentSelectionTest {
    private fun block(id: Long, x: Double, y: Double, width: Double = 40.0, height: Double = 12.0): ContentTextBlock {
        val bounds = AnnotationRect(x, y, width, height)
        return ContentTextBlock(listOf(ContentTextRun(id, 0, bounds, "F1", ContentFontKind.Standard14, "t$id")), bounds, "t$id")
    }

    private fun image(id: Long, x: Double, y: Double) = ContentImage(id, 0, AnnotationRect(x, y, 30.0, 30.0), "Im$id")

    private fun path(id: Long, x: Double, y: Double, width: Double, height: Double) =
        ContentPath(id, 0, AnnotationRect(x, y, width, height), PathPaint.Stroke)

    private val title = block(1, 10.0, 150.0)
    private val total = block(2, 10.0, 120.0)
    private val logo = image(3, 10.0, 20.0)
    private val rule = path(4, 10.0, 80.0, 60.0, 0.0)

    @Test
    fun aMarqueeIsTheBoxBetweenItsTwoCornersWhicheverWayItWasDragged() {
        val expected = AnnotationRect(10.0, 20.0, 30.0, 40.0)

        assertEquals(expected, marqueeRect(AnnotationPoint(10.0, 20.0), AnnotationPoint(40.0, 60.0)))
        assertEquals(expected, marqueeRect(AnnotationPoint(40.0, 60.0), AnnotationPoint(10.0, 20.0)))
        assertEquals(expected, marqueeRect(AnnotationPoint(40.0, 20.0), AnnotationPoint(10.0, 60.0)))
    }

    @Test
    fun touchingIsInclusiveSoABoxMeetingOnlyAnEdgeCounts() {
        val box = AnnotationRect(0.0, 0.0, 10.0, 10.0)

        assertTrue(box.touches(AnnotationRect(5.0, 5.0, 2.0, 2.0)))
        assertTrue(box.touches(AnnotationRect(10.0, 10.0, 5.0, 5.0)))
        assertFalse(box.touches(AnnotationRect(10.5, 0.0, 5.0, 5.0)))
        assertFalse(box.touches(AnnotationRect(0.0, -6.0, 5.0, 5.0)))
    }

    @Test
    fun aHairlineRuleIsGatheredByAMarqueeThatOnlyCrossesIt() {
        val area = AnnotationRect(20.0, 79.0, 10.0, 2.0)

        val selection = selectionIn(0, listOf(title, total), listOf(logo), listOf(rule), area)

        assertEquals(listOf(4L), selection?.paths?.map { it.id })
        assertTrue(selection?.blocks?.isEmpty() == true && selection.images.isEmpty())
    }

    @Test
    fun aMarqueeGathersWhatItTouchesOfEveryKind() {
        val selection = selectionIn(0, listOf(title, total), listOf(logo), listOf(rule), AnnotationRect(0.0, 0.0, 100.0, 200.0))

        assertEquals(0, selection?.pageIndex)
        assertEquals(listOf(1L, 2L), selection?.blocks?.map { it.id })
        assertEquals(listOf(3L), selection?.images?.map { it.id })
        assertEquals(listOf(4L), selection?.paths?.map { it.id })
        assertEquals(4, selection?.count)
    }

    @Test
    fun aMarqueeOverNothingGathersNothing() {
        assertNull(selectionIn(0, listOf(title), listOf(logo), listOf(rule), AnnotationRect(60.0, 170.0, 10.0, 10.0)))
    }

    @Test
    fun aTapOnAMemberTakesItOutAndOnANonMemberAddsIt() {
        val start = ContentSelection(0, blocks = listOf(title))

        val added = start.toggled(ContentTarget.Image(logo))
        assertEquals(listOf(3L), added?.images?.map { it.id })
        assertEquals(listOf(1L), added?.blocks?.map { it.id })

        val removed = added?.toggled(ContentTarget.Block(title))
        assertTrue(removed?.blocks?.isEmpty() == true)
        assertEquals(listOf(3L), removed?.images?.map { it.id })
    }

    @Test
    fun takingOutTheLastMemberEmptiesTheSelection() {
        assertNull(ContentSelection(0, paths = listOf(rule)).toggled(ContentTarget.Path(rule)))
    }

    @Test
    fun membershipIsByIdSoAMovedMemberIsStillOne() {
        val selection = ContentSelection(0, images = listOf(logo))

        assertTrue(ContentTarget.Image(logo.copy(bounds = AnnotationRect(0.0, 0.0, 1.0, 1.0))) in selection)
        assertFalse(ContentTarget.Block(title) in selection)
    }

    @Test
    fun theUnionBoxCoversEveryMemberAndPadsAHairline() {
        val selection = ContentSelection(0, blocks = listOf(total), paths = listOf(rule))

        // total spans y 120–132; the rule is flat at y 80, padded by PATH_PAD above and below.
        assertEquals(AnnotationRect(10.0 - PATH_PAD, 80.0 - PATH_PAD, 60.0 + 2 * PATH_PAD, 132.0 - (80.0 - PATH_PAD)), selection.bounds)
    }

    @Test
    fun theUnionBoxOfOneImageIsTheImage() {
        assertEquals(logo.bounds, ContentSelection(0, images = listOf(logo)).bounds)
    }

    @Test
    fun aMovedSelectionIsReResolvedByIdFromTheFreshRead() {
        val selection = ContentSelection(0, blocks = listOf(title), images = listOf(logo), paths = listOf(rule))
        val movedLogo = logo.copy(bounds = AnnotationRect(15.0, 10.0, 30.0, 30.0))
        val movedTitle = title.copy(bounds = AnnotationRect(15.0, 140.0, 40.0, 12.0))

        val now = selection.reread(listOf(movedTitle, total), listOf(movedLogo), listOf(rule))

        assertEquals(movedTitle.bounds, now?.blocks?.single()?.bounds)
        assertEquals(movedLogo.bounds, now?.images?.single()?.bounds)
        assertEquals(rule, now?.paths?.single())
    }

    @Test
    fun aMemberTheReadNoLongerReportsDropsOutAndAnEmptyResultIsNull() {
        val selection = ContentSelection(0, blocks = listOf(title), images = listOf(logo))

        assertEquals(listOf(1L), selection.reread(listOf(title), emptyList(), emptyList())?.blocks?.map { it.id })
        assertNull(selection.reread(emptyList(), emptyList(), emptyList()))
    }

    @Test
    fun aFingerThatBarelyMovedIsAPressNotAMarquee() {
        val origin = AnnotationPoint(50.0, 50.0)

        assertTrue(isPress(origin, AnnotationPoint(52.0, 51.0), slop = 5.0))
        assertFalse(isPress(origin, AnnotationPoint(60.0, 50.0), slop = 5.0))
    }

    @Test
    fun theCountReadsInTheSingularAndThePlural() {
        assertTrue(selectedMessage(1).startsWith("1 item selected"))
        assertTrue(selectedMessage(3).startsWith("3 items selected"))
    }
}
