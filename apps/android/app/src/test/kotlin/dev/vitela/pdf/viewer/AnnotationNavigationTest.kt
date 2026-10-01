package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Stepping through the snapshot's annotations, and where the reader scrolls to show one. */
class AnnotationNavigationTest {
    private val annotations = listOf(annotation(7, 2), annotation(3, 0), annotation(9, 1))

    @Test
    fun stepsFollowSnapshotOrderAndWrapBothWays() {
        assertEquals(9L, annotationStep(annotations, 3, forward = true)?.id)
        assertEquals(7L, annotationStep(annotations, 9, forward = true)?.id)
        assertEquals(9L, annotationStep(annotations, 7, forward = false)?.id)
        assertEquals(7L, annotationStep(annotations, 3, forward = false)?.id)
    }

    @Test
    fun withoutASelectionNextEntersAtTheFirstAndPreviousAtTheLast() {
        assertEquals(7L, annotationStep(annotations, null, forward = true)?.id)
        assertEquals(9L, annotationStep(annotations, null, forward = false)?.id)
    }

    @Test
    fun emptySingletonAndStaleSnapshots() {
        assertNull(annotationStep(emptyList(), null, forward = true))
        assertEquals(7L, annotationStep(listOf(annotation(7, 0)), 7, forward = true)?.id)
        assertEquals(7L, annotationStep(listOf(annotation(7, 0)), 7, forward = false)?.id)
        assertNull(annotationStep(annotations, 42, forward = true))
    }

    @Test
    fun navigationNeedsAnnotationsAndTheReader() {
        val ready = ViewerState(pageCount = 3, annotations = annotations)
        assertTrue(annotationNavigationEnabled(ready))
        assertTrue(annotationNavigationEnabled(ready.copy(annotationEditingAllowed = false)))
        assertFalse(annotationNavigationEnabled(ready.copy(annotations = emptyList())))
        assertFalse(annotationNavigationEnabled(ready.copy(isLoading = true)))
        assertFalse(annotationNavigationEnabled(ready.copy(organize = OrganizeState())))
        assertFalse(annotationNavigationEnabled(ready.copy(contentEdit = ContentEditState())))
    }

    @Test
    fun revealCentresASmallRectInTheViewport() {
        val reveal = revealScroll(PlacedRect(left = 900.0, top = 1200.0, width = 100.0, height = 40.0), viewportWidthPx = 400, viewportHeightPx = 600)
        assertEquals(RevealScroll(itemOffsetPx = 920, scrollXPx = 750), reveal)
    }

    @Test
    fun revealNeverScrollsAboveThePageOrLeftOfTheColumn() {
        val reveal = revealScroll(PlacedRect(left = 10.0, top = 20.0, width = 30.0, height = 10.0), viewportWidthPx = 400, viewportHeightPx = 600)
        assertEquals(RevealScroll(itemOffsetPx = 0, scrollXPx = 0), reveal)
    }

    @Test
    fun aRectLargerThanTheViewportShowsItsTopLeftCorner() {
        val reveal = revealScroll(PlacedRect(left = 500.0, top = 300.0, width = 800.0, height = 900.0), viewportWidthPx = 400, viewportHeightPx = 600)
        assertEquals(RevealScroll(itemOffsetPx = 300, scrollXPx = 500), reveal)
    }

    @Test
    fun inkIsRevealedByItsStrokeBounds() {
        val ink = Annotation(1, 0, AnnotationKind.Ink, null, DEFAULT_ANNOTATION_COLOR, listOf(AnnotationPoint(10.0, 20.0), AnnotationPoint(50.0, 60.0)))
        assertEquals(AnnotationReveal(0, AnnotationRect(10.0, 20.0, 40.0, 40.0)), annotationReveal(ink))
    }

    private fun annotation(id: Long, pageIndex: Int) =
        Annotation(id, pageIndex, AnnotationKind.Highlight, AnnotationRect(10.0, 20.0, 30.0, 8.0), DEFAULT_ANNOTATION_COLOR)
}
