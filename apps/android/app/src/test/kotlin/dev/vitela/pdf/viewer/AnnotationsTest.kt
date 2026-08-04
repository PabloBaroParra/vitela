package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationColor
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.TextRect
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AnnotationsTest {
    @Test
    fun composePdfConversion_flipsTheBottomLeftPdfAxisAtAnyZoom() {
        val point = screenToPdf(AnnotationPoint(150.0, 75.0), pageHeightPt = 792.0, scale = 1.5)
        assertEquals(100.0, point.x, 0.0)
        assertEquals(742.0, point.y, 0.0)
        assertEquals(AnnotationPoint(150.0, 75.0), pdfToScreen(point, pageHeightPt = 792.0, scale = 1.5))
    }

    @Test
    fun placement_createsMarkupAndFreehandWithNormalizedPdfGeometry() {
        val highlight = placementAnnotation(AnnotationTool.Highlight, 4, AnnotationPoint(300.0, 400.0), AnnotationPoint(120.0, 340.0))
        val ink = placementAnnotation(AnnotationTool.Ink, 4, AnnotationPoint(10.0, 20.0), AnnotationPoint(30.0, 40.0), listOf(AnnotationPoint(10.0, 20.0), AnnotationPoint(30.0, 40.0)))
        assertEquals(AnnotationKind.Highlight, highlight.kind)
        assertEquals(AnnotationRect(120.0, 340.0, 180.0, 60.0), highlight.rect)
        assertEquals(AnnotationKind.Ink, ink.kind)
        assertEquals(2, ink.points.size)
    }

    @Test
    fun textSelection_createsOneMarkupPerLineAndClearsSelection() {
        val selection = TextSelection(2, listOf(TextRect(10.0, 20.0, 30.0, 8.0), TextRect(10.0, 8.0, 25.0, 8.0)))
        val annotations = markupTextSelection(AnnotationTool.Underline, selection)
        assertEquals(2, annotations.size)
        assertTrue(annotations.all { it.kind == AnnotationKind.Underline && it.pageIndex == 2 })
    }

    @Test
    fun selectionAndResize_prioritizeHandlesAndNeverResizeInk() {
        val rectangle = Annotation(9, 0, AnnotationKind.Highlight, AnnotationRect(10.0, 20.0, 40.0, 30.0), DEFAULT_ANNOTATION_COLOR)
        val ink = Annotation(10, 0, AnnotationKind.Ink, null, DEFAULT_ANNOTATION_COLOR, listOf(AnnotationPoint(10.0, 20.0), AnnotationPoint(50.0, 50.0)))
        assertEquals(DragMode.Resize(HandleCorner.TopRight), dragModeAt(rectangle, AnnotationPoint(50.0, 50.0), 2.0))
        assertEquals(DragMode.Move, dragModeAt(ink, AnnotationPoint(50.0, 50.0), 2.0))
        assertEquals(AnnotationRect(10.0, 20.0, 50.0, 40.0), resizedRect(rectangle.rect!!, HandleCorner.TopRight, AnnotationPoint(60.0, 60.0)))
    }

    @Test
    fun growMoveAndRestyleOnlyApplyToSupportedSelection() {
        val annotation = Annotation(1, 0, AnnotationKind.Highlight, AnnotationRect(10.0, 20.0, 40.0, 30.0), DEFAULT_ANNOTATION_COLOR)
        assertEquals(AnnotationRect(10.0, 20.0, 50.0, 37.5), grownRect(annotation.rect!!))
        assertEquals(AnnotationRect(22.0, 32.0, 40.0, 30.0), movedRect(annotation.rect, 12.0, 12.0))
        assertTrue(annotation.supportsRestyle)
        assertFalse(Annotation(2, 0, AnnotationKind.TextNote, annotation.rect, null).supportsRestyle)
    }

    @Test
    fun controls_followPermissionSelectionAndHistory() {
        assertEquals(AnnotationControls.disabled, annotationControls(editingAllowed = false, selected = null, canUndo = true, canRedo = true))
        val selected = Annotation(1, 0, AnnotationKind.Highlight, AnnotationRect(0.0, 0.0, 4.0, 4.0), AnnotationColor(1, 2, 3))
        val controls = annotationControls(editingAllowed = true, selected, canUndo = true, canRedo = false)
        assertTrue(controls.canCreate && controls.canMove && controls.canResize && controls.canRestyle && controls.canGrow && controls.canUndo)
        assertFalse(controls.canRedo)
    }

    @Test
    fun hitReach_isScreenStableAcrossZoomLevels() {
        assertEquals(24.0, handleReachPoints(screenReachDp = 24.0, density = 1.0, pageScale = 1.0), 0.0)
        assertEquals(12.0, handleReachPoints(screenReachDp = 24.0, density = 1.0, pageScale = 2.0), 0.0)
    }

    @Test
    fun annotationAt_returnsTheTopmostMatchingAnnotation() {
        val bottom = Annotation(1, 0, AnnotationKind.Shape, AnnotationRect(0.0, 0.0, 10.0, 10.0), DEFAULT_ANNOTATION_COLOR)
        val top = Annotation(2, 0, AnnotationKind.TextNote, AnnotationRect(0.0, 0.0, 10.0, 10.0), null)
        assertEquals(top, annotationAt(listOf(bottom, top), 0, AnnotationPoint(5.0, 5.0)))
    }

    @Test
    fun placement_createsShapeAndTextNoteWithTheirCoreSupportedProperties() {
        val shape = placementAnnotation(AnnotationTool.Shape, 0, AnnotationPoint(10.0, 20.0), AnnotationPoint(30.0, 50.0))
        val note = placementAnnotation(AnnotationTool.TextNote, 0, AnnotationPoint(10.0, 20.0), AnnotationPoint(30.0, 50.0))

        assertEquals(AnnotationKind.Shape, shape.kind)
        assertEquals(DEFAULT_ANNOTATION_COLOR, shape.color)
        assertEquals(AnnotationKind.TextNote, note.kind)
        assertEquals(null, note.color)
    }
}
