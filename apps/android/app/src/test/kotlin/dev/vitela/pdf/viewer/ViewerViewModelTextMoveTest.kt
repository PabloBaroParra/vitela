package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.PdfCoreError
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

/** Moving a line of text the page itself paints, from the retype dialog: Move arms a tap, the tap is one undoable edit. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelTextMoveTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // RetypableDocument's "Hello" fills (10, 150)–(50, 162) on page 0; nothing is painted at (50, 180).
    private val onHello = AnnotationPoint(20.0, 155.0)
    private val clear = AnnotationPoint(50.0, 180.0)

    @Test
    fun moveClosesTheDialogAndArmsATap() = runTest {
        val viewModel = armed(RetypableDocument())

        val state = viewModel.state.value
        assertNull(state.contentEdit?.editor)
        assertEquals(10L, state.contentEdit?.movingText?.id)
        assertNull("one move is armed at a time", state.contentEdit?.movingImage)
        assertEquals(textMovePrompt(0), state.status)
    }

    @Test
    fun theArmedTapTakesTheRunsTopLeftCornerKeepingItsSize() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        assertEquals(listOf(10L to AnnotationRect(50.0, 168.0, 40.0, 12.0)), document.textMoves.map { (run, to) -> run.id to to })
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(TEXT_MOVED, state.status)
        assertNull("one tap, one move", state.contentEdit?.movingText)
        assertEquals(
            "the outline must describe where the run now is",
            AnnotationRect(50.0, 168.0, 40.0, 12.0),
            state.contentEdit?.runs?.get(0)?.single { it.id == 10L }?.bounds,
        )
    }

    @Test
    fun aMoveRedrawsThePage() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        advanceUntilIdle()
        val renders = document.drawn.size
        arm(viewModel)
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        assertTrue("only the renderer can paint the moved text", document.drawn.drop(renders).any { it.first == 0 })
    }

    @Test
    fun aTapOnAnotherPageMovesNothingAndStaysArmed() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()

        assertTrue(document.textMoves.isEmpty())
        assertEquals(10L, viewModel.state.value.contentEdit?.movingText?.id)
        assertEquals(textMovePrompt(0), viewModel.state.value.status)
    }

    @Test
    fun aDoubleTapQueuesOneMove() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.tapContent(0, clear, reach = 0.0)
        viewModel.tapContent(0, AnnotationPoint(60.0, 190.0), reach = 0.0)
        advanceUntilIdle()

        assertEquals("the second tap found the move already spent", 1, document.textMoves.size)
    }

    @Test
    fun aTapOnTheCornerItAlreadyHasQueuesNothing() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.tapContent(0, AnnotationPoint(10.0, 162.0), reach = 0.0)
        advanceUntilIdle()

        assertTrue(document.textMoves.isEmpty())
        val state = viewModel.state.value
        assertFalse(state.isDirty)
        assertNull(state.contentEdit?.movingText)
        assertEquals(TEXT_POSITION_UNCHANGED, state.status)
    }

    @Test
    fun aRefusedMoveSaysWhyAndDisarms() = runTest {
        val refusal = "This text cannot be moved."
        val viewModel = armed(RetypableDocument(refusal = PdfCoreError.Failed(refusal)))
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(refusal, state.status)
        assertNull(state.contentEdit?.movingText)
        assertFalse(state.isDirty)
    }

    @Test
    fun undoingAMoveRereadsTheRuns() = runTest {
        val viewModel = armed(RetypableDocument())
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(AnnotationRect(10.0, 150.0, 40.0, 12.0), viewModel.state.value.contentEdit?.runs?.get(0)?.single { it.id == 10L }?.bounds)
    }

    @Test
    fun anUndoDisarmsAPendingMove() = runTest {
        val viewModel = armed(RetypableDocument())
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()
        // Armed again on the run where it now is — which the undo takes back.
        viewModel.tapContent(0, AnnotationPoint(60.0, 175.0), reach = 0.0)
        advanceUntilIdle()
        viewModel.armTextMove(viewModel.state.value.documentId)
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertNull("the run it held may no longer be where it was", viewModel.state.value.contentEdit?.movingText)
    }

    @Test
    fun cancellingAMoveLeavesTheNextTapToOpenWhatItHits() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.cancelContentMove()
        assertEquals(MOVE_CANCELLED, viewModel.state.value.status)
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()

        assertTrue(document.textMoves.isEmpty())
        assertEquals(10L, viewModel.state.value.contentEdit?.editor?.run?.id)
    }

    @Test
    fun armingAnInsertDisarmsAMove() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.armTextInsert()
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        assertTrue("the tap went to the insert", document.textMoves.isEmpty())
        assertNull(viewModel.state.value.contentEdit?.movingText)
    }

    @Test
    fun moveFromADialogBuiltForAReplacedDocumentArmsNothing() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.openContentEdit()
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.armTextMove(viewModel.state.value.documentId + 1)

        assertNull(viewModel.state.value.contentEdit?.movingText)
        assertEquals(10L, viewModel.state.value.contentEdit?.editor?.run?.id)
    }

    private suspend fun TestScope.armed(document: PdfDocument): ViewerViewModel = openedWith(document).also { arm(it) }

    private fun TestScope.arm(viewModel: ViewerViewModel) {
        viewModel.openContentEdit()
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.armTextMove(viewModel.state.value.documentId)
    }

    private suspend fun TestScope.openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
