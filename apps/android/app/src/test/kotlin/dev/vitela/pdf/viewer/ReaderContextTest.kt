package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.ContentPath
import dev.vitela.pdf.core.ContentSelection
import dev.vitela.pdf.core.PathPaint
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ReaderContextTest {
    private val highlight = Annotation(7, 0, AnnotationKind.Highlight, AnnotationRect(10.0, 20.0, 30.0, 8.0), DEFAULT_ANNOTATION_COLOR)

    private fun chips(state: ViewerState): List<ContextChip> {
        val selected = state.annotations.lastOrNull { it.id == state.selectedAnnotationId }
        return contextChips(state, annotationControls(state.annotationEditingAllowed, selected, false, false))
    }

    @Test
    fun nothingSelectedShowsNoChips() {
        assertTrue(chips(ViewerState(pageCount = 1, annotationEditingAllowed = true)).isEmpty())
    }

    @Test
    fun selectedTextLeavesItsActionsToTheSelectionToolbar() {
        // Copy, markup and share sit on the floating bar over the selection, not in the chip row.
        val state = ViewerState(pageCount = 1, textSelection = TextSelection(0, emptyList(), "hello", 0, 5))

        assertTrue(chips(state).isEmpty())
    }

    @Test
    fun anEditableSelectedHighlightOffersEveryEditInOrder() {
        val state = ViewerState(pageCount = 1, annotations = listOf(highlight), selectedAnnotationId = 7, annotationEditingAllowed = true)

        assertEquals(
            listOf(ContextChip.Grow, ContextChip.Resize, ContextChip.MoveTo, ContextChip.Red, ContextChip.Gold, ContextChip.Delete),
            chips(state),
        )
    }

    @Test
    fun aSelectedAnnotationIsNotDeletableWithoutThePermission() {
        val state = ViewerState(pageCount = 1, annotations = listOf(highlight), selectedAnnotationId = 7, annotationEditingAllowed = false)

        assertTrue(chips(state).isEmpty())
    }

    @Test
    fun anArmedMoveOrInsertCanBeCancelled() {
        val adding = ViewerState(pageCount = 1, contentEdit = ContentEditState(adding = ContentAddition.Text))

        assertEquals(listOf(ContextChip.CancelInsert), chips(adding))
    }

    @Test
    fun aSelectionOfSeveralItemsOffersItsCountCopyCutDeleteAndClear() {
        val selection = ContentSelection(0, images = listOf(ContentImage(30, 0, AnnotationRect(10.0, 20.0, 30.0, 30.0), "Im1")), paths = listOf(ContentPath(40, 0, AnnotationRect(10.0, 80.0, 60.0, 0.0), PathPaint.Stroke)))
        val state = ViewerState(pageCount = 1, contentEdit = ContentEditState(selection = selection))

        assertEquals(listOf(ContextChip.Selected, ContextChip.CopySelection, ContextChip.CutSelection, ContextChip.DeleteSelection, ContextChip.ClearSelection), chips(state))
        assertEquals("2 selected", ContextChip.Selected.labelFor(state))
        assertEquals("Delete", ContextChip.DeleteSelection.labelFor(state))
        assertEquals("Copy", ContextChip.CopySelection.labelFor(state))
        assertEquals("Cut", ContextChip.CutSelection.labelFor(state))
    }
}
