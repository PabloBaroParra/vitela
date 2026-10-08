package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.FreeTextLayout
import dev.vitela.pdf.core.FreeTextLine
import dev.vitela.pdf.core.PageRotation
import dev.vitela.pdf.core.PageSize
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The pure half of FreeText: where a new box lands, what the overlay draws
 * (the core's lines, never a re-wrap) and how it is handled once selected.
 */
class FreeTextTest {
    private val letter = PageSize(612.0, 792.0)

    @Test
    fun aTapPlacesThe200By50BoxWithItsTopLeftOnTheTap() {
        assertEquals(AnnotationRect(100.0, 650.0, 200.0, 50.0), freeTextPlacementRect(AnnotationPoint(100.0, 700.0), letter))
    }

    @Test
    fun aTapNearTheRightEdgeSlidesTheBoxBackOntoThePage() {
        assertEquals(AnnotationRect(412.0, 650.0, 200.0, 50.0), freeTextPlacementRect(AnnotationPoint(600.0, 700.0), letter))
    }

    @Test
    fun aTapNearTheTopOrBottomEdgeKeepsTheWholeBoxOnThePage() {
        assertEquals(742.0, freeTextPlacementRect(AnnotationPoint(100.0, 795.0), letter).y, 0.0)
        assertEquals(0.0, freeTextPlacementRect(AnnotationPoint(100.0, 10.0), letter).y, 0.0)
    }

    @Test
    fun aPageSmallerThanTheBoxShrinksTheBoxToThePage() {
        assertEquals(AnnotationRect(0.0, 0.0, 100.0, 40.0), freeTextPlacementRect(AnnotationPoint(30.0, 30.0), PageSize(100.0, 40.0)))
    }

    @Test
    fun aTurnedPageIsClampedAgainstItsUnrotatedSpaceWhereTheBoxLives() {
        val turned = PageSize(792.0, 612.0, PageRotation.Clockwise90)

        assertEquals(AnnotationRect(412.0, 650.0, 200.0, 50.0), freeTextPlacementRect(AnnotationPoint(600.0, 700.0), turned))
    }

    private val lines = listOf(FreeTextLine("Canción de", 2.0, 12.6), FreeTextLine("cuna ¿qué?", 2.0, 26.4))
    private val note = Annotation(
        7, 0, AnnotationKind.FreeText, AnnotationRect(10.0, 50.0, 40.0, 20.0), null,
        contents = "Canción de cuna ¿qué?", layout = FreeTextLayout(12.0, lines),
    )

    @Test
    fun theOverlayDrawsTheCoresLinesVerbatimAccentsIncluded() {
        val drawing = requireNotNull(freeTextDrawing(note, PagePlacement(PageSize(100.0, 200.0), 1.0)))

        assertEquals(listOf("Canción de", "cuna ¿qué?"), drawing.lines.map { it.text })
        assertEquals(lines, drawing.lines)
        assertEquals(12.0, drawing.fontSizePt, 0.0)
        assertEquals(40.0, drawing.width, 0.0)
        assertEquals(20.0, drawing.height, 0.0)
    }

    @Test
    fun anUpright1xPageMapsTheBoxFrameStraightOntoItsTopLeft() {
        val drawing = requireNotNull(freeTextDrawing(note, PagePlacement(PageSize(100.0, 200.0), 1.0)))

        // PDF top edge is y = 70; page height 200 puts it 130 down.
        assertEquals(FreeTextFrame(1.0, 0.0, 0.0, 1.0, 10.0, 130.0), drawing.frame)
    }

    @Test
    fun zoomScalesTheFrameSoFontAndLinePositionsScaleWithIt() {
        val drawing = requireNotNull(freeTextDrawing(note, PagePlacement(PageSize(100.0, 200.0), 2.0)))

        assertEquals(FreeTextFrame(2.0, 0.0, 0.0, 2.0, 20.0, 260.0), drawing.frame)
        // The lines stay in the box's own points: the frame is what multiplies them.
        assertEquals(lines, drawing.lines)
    }

    @Test
    fun aQuarterTurnedPageLaysTheTextOnItsSideLikeTheSavedFile() {
        val drawing = requireNotNull(freeTextDrawing(note, PagePlacement(PageSize(200.0, 100.0, PageRotation.Clockwise90), 1.0)))

        assertEquals(FreeTextFrame(0.0, 1.0, -1.0, 0.0, 70.0, 10.0), drawing.frame)
    }

    @Test
    fun aHalfTurnedPageInvertsBothAxes() {
        val drawing = requireNotNull(freeTextDrawing(note, PagePlacement(PageSize(100.0, 200.0, PageRotation.Clockwise180), 1.0)))

        assertEquals(FreeTextFrame(-1.0, 0.0, 0.0, -1.0, 90.0, 70.0), drawing.frame)
    }

    @Test
    fun aThreeQuarterTurnedPageMapsTheFrameThroughTheOtherQuarterTurn() {
        val drawing = requireNotNull(freeTextDrawing(note, PagePlacement(PageSize(200.0, 100.0, PageRotation.Clockwise270), 1.0)))

        // placePoint(x, y) = (pageWidth - y, pageHeight - x) on the drawn 200x100 page.
        assertEquals(FreeTextFrame(0.0, -1.0, 1.0, 0.0, 130.0, 90.0), drawing.frame)
    }

    @Test
    fun onlyAFreeTextWithALayoutHasAnythingToDraw() {
        val placement = PagePlacement(PageSize(100.0, 200.0), 1.0)

        assertNull(freeTextDrawing(note.copy(kind = AnnotationKind.TextNote), placement))
        assertNull(freeTextDrawing(note.copy(layout = null), placement))
    }

    @Test
    fun aDragBelowTheMinimumBoxIsClampedNeverDegenerate() {
        val box = note.copy(rect = AnnotationRect(100.0, 100.0, 100.0, 50.0))
        val highlight = Annotation(8, 0, AnnotationKind.Highlight, AnnotationRect(100.0, 100.0, 100.0, 50.0), DEFAULT_ANNOTATION_COLOR)

        assertEquals(AnnotationRect(100.0, 100.0, 16.0, 17.8), annotationResizedRect(box, HandleCorner.TopRight, AnnotationPoint(101.0, 101.0)))
        // The other kinds keep their own 4 pt floor.
        assertEquals(AnnotationRect(100.0, 100.0, 4.0, 4.0), annotationResizedRect(highlight, HandleCorner.TopRight, AnnotationPoint(101.0, 101.0)))
    }

    @Test
    fun aDragAboveTheMinimumResizesExactlyWhereTheFingerIs() {
        val box = note.copy(rect = AnnotationRect(100.0, 100.0, 100.0, 50.0))

        assertEquals(AnnotationRect(100.0, 100.0, 60.0, 40.0), annotationResizedRect(box, HandleCorner.TopRight, AnnotationPoint(160.0, 140.0)))
    }

    @Test
    fun editTextIsOfferedFirstForAnEditableSelectedFreeText() {
        val state = ViewerState(pageCount = 1, annotations = listOf(note), selectedAnnotationId = 7, annotationEditingAllowed = true)
        val controls = annotationControls(true, note, false, false)

        assertEquals(
            listOf(ContextChip.EditText, ContextChip.Grow, ContextChip.Resize, ContextChip.MoveTo, ContextChip.Delete),
            contextChips(state, controls),
        )
    }

    @Test
    fun editTextIsNotOfferedOnAReadOnlyDocumentOrForAnotherKind() {
        val readOnly = ViewerState(pageCount = 1, annotations = listOf(note), selectedAnnotationId = 7, annotationEditingAllowed = false)
        val highlight = Annotation(8, 0, AnnotationKind.Highlight, AnnotationRect(1.0, 1.0, 5.0, 5.0), DEFAULT_ANNOTATION_COLOR)
        val other = ViewerState(pageCount = 1, annotations = listOf(highlight), selectedAnnotationId = 8, annotationEditingAllowed = true)

        assertTrue(contextChips(readOnly, annotationControls(false, note, false, false)).isEmpty())
        assertTrue(ContextChip.EditText !in contextChips(other, annotationControls(true, highlight, false, false)))
    }

    @Test
    fun addIsEnabledOnlyForNonBlankTextWhileTheCoreIsNotAnswering() {
        assertTrue(freeTextConfirmEnabled("x", busy = false))
        assertFalse(freeTextConfirmEnabled("", busy = false))
        assertFalse(freeTextConfirmEnabled("  \n\t", busy = false))
        assertFalse(freeTextConfirmEnabled("x", busy = true))
    }

    @Test
    fun theDialogsWordingNamesWhatItDoes() {
        val place = FreeTextDraft(2, FreeTextTarget.Place(AnnotationRect(0.0, 0.0, 1.0, 1.0)))
        val retype = FreeTextDraft(2, FreeTextTarget.Retype(7, "uno"))

        assertEquals("Add text box — page 3", freeTextDialogTitle(place))
        assertEquals("Edit text — page 3", freeTextDialogTitle(retype))
        assertEquals("Add", freeTextConfirmLabel(place))
        assertEquals("Save", freeTextConfirmLabel(retype))
        assertEquals("", freeTextInitialText(place))
        assertEquals("uno", freeTextInitialText(retype))
    }

    @Test
    fun aFreeTextBoxCannotBeRecoloured() {
        assertTrue(!note.supportsRestyle)
        assertTrue(note.supportsResize)
    }
}
