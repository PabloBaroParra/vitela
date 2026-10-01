package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationEdit
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Placing a Note asks for its text first (Windows #293): the tap or drag only
 * chooses the rectangle, and nothing reaches the core until **Add**. Cancel
 * leaves no annotation and no undo step.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelNotePlacementTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun placingANoteAsksForTextBeforeAnyEdit() = runTest {
        val (viewModel, document) = opened()
        place(viewModel)
        advanceUntilIdle()

        assertEquals(NotePlacement(0, AnnotationRect(10.0, 20.0, 30.0, 40.0)), viewModel.state.value.notePlacement)
        assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun addRecordsTheTypedTextVerbatimAsOneUndoableEdit() = runTest {
        val (viewModel, document) = opened()
        place(viewModel)
        viewModel.addNote(viewModel.state.value.documentId, "  First line\nSecond line  ")
        advanceUntilIdle()

        val added = (document.edits.single() as AnnotationEdit.Add).annotation
        assertEquals(AnnotationKind.TextNote, added.kind)
        assertEquals(AnnotationRect(10.0, 20.0, 30.0, 40.0), added.rect)
        assertEquals("  First line\nSecond line  ", added.contents)
        assertNull(viewModel.state.value.notePlacement)
        assertTrue(viewModel.state.value.canUndoAnnotations)
        assertTrue(viewModel.state.value.isDirty)
    }

    @Test
    fun cancelLeavesNoAnnotationAndNoUndoStep() = runTest {
        val (viewModel, document) = opened()
        place(viewModel)
        viewModel.cancelNote()
        advanceUntilIdle()

        assertNull(viewModel.state.value.notePlacement)
        assertEquals(NOTE_PLACEMENT_CANCELED, viewModel.state.value.status)
        assertTrue(document.edits.isEmpty())
        assertFalse(viewModel.state.value.canUndoAnnotations)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun blankTextAddsNothingAndKeepsThePrompt() = runTest {
        val (viewModel, document) = opened()
        place(viewModel)
        viewModel.addNote(viewModel.state.value.documentId, " \n\t")
        advanceUntilIdle()

        assertTrue(document.edits.isEmpty())
        assertEquals(0, viewModel.state.value.notePlacement?.pageIndex)
    }

    @Test
    fun aPromptBuiltForAnotherDocumentAddsNothing() = runTest {
        val (viewModel, document) = opened()
        place(viewModel)
        viewModel.addNote(viewModel.state.value.documentId - 1, "Stale")
        advanceUntilIdle()

        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun otherToolsStillPlaceImmediately() = runTest {
        val (viewModel, document) = opened()
        viewModel.setAnnotationTool(AnnotationTool.Shape)
        viewModel.placeAnnotation(0, AnnotationPoint(10.0, 20.0), AnnotationPoint(40.0, 60.0))
        advanceUntilIdle()

        assertEquals(AnnotationKind.Shape, (document.edits.single() as AnnotationEdit.Add).annotation.kind)
        assertNull(viewModel.state.value.notePlacement)
    }

    private fun place(viewModel: ViewerViewModel) {
        viewModel.setAnnotationTool(AnnotationTool.TextNote)
        viewModel.placeAnnotation(0, AnnotationPoint(10.0, 20.0), AnnotationPoint(40.0, 60.0))
    }

    private suspend fun TestScope.opened(): Pair<ViewerViewModel, NoteDocument> {
        val document = NoteDocument()
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel to document
    }
}

/** Records every annotation edit; one queued edit is one undo step, like the core's log. */
private class NoteDocument(private val base: PdfDocument = RetypableDocument()) : PdfDocument by base {
    val edits = mutableListOf<AnnotationEdit>()

    override fun annotations() = PdfCoreResult.Success(AnnotationSnapshot(emptyList(), true, edits.isNotEmpty(), false))

    override fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> {
        edits += edit
        return PdfCoreResult.Success(Unit)
    }
}
