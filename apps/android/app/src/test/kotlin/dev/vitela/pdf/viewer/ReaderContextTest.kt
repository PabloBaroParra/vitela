package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationRect
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
    fun selectedTextOffersCopyOnly() {
        val state = ViewerState(pageCount = 1, textSelection = TextSelection(0, emptyList(), "hello"))

        assertEquals(listOf(ContextChip.Copy), chips(state))
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
}
