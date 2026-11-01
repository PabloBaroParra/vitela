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

/**
 * Editing a line the page paints in several runs as one block (B25 T-207):
 * RetypableDocument's page 1 paints "Two" and "runs" a word gap apart, which the
 * core groups into one block — one target, one undo step for every edit.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelTextBlockTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // "Two" fills (10, 100)–(40, 112) on page 1, "runs" (42, 100)–(82, 112); nothing is painted at (50, 180).
    private val onTwo = AnnotationPoint(20.0, 105.0)
    private val onRuns = AnnotationPoint(60.0, 105.0)
    private val inTheGap = AnnotationPoint(41.0, 105.0)
    private val clear = AnnotationPoint(50.0, 180.0)

    @Test
    fun aPageIsOutlinedByBlockNotByRun() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.openContentEdit()
        viewModel.contentPageShown(1)
        advanceUntilIdle()

        assertEquals(listOf("Page two", "Two runs"), viewModel.state.value.contentEdit?.blocks?.get(1)?.map { it.text })
    }

    @Test
    fun aTapOnAnyRunOfALineOpensTheWholeLine() = runTest {
        for (tap in listOf(onTwo, onRuns, inTheGap)) {
            val viewModel = openedWith(RetypableDocument())
            viewModel.openContentEdit()
            viewModel.tapContent(1, tap, reach = 0.0)
            advanceUntilIdle()

            val editor = viewModel.state.value.contentEdit?.editor
            assertEquals(listOf(21L, 22L), editor?.block?.runs?.map { it.id })
            assertEquals("the dialog edits the line as one piece of text", "Two runs", editor?.text)
        }
    }

    @Test
    fun retypingALineSendsEveryRunAndLeavesOneRun() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "Three whole words")
        advanceUntilIdle()

        val (sent, text) = document.retypes.single()
        assertEquals(listOf(21L, 22L), sent.runs.map { it.id })
        assertEquals("Three whole words", text)
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertEquals(TEXT_UPDATED, state.status)
        assertNull(state.contentEdit?.editor)
        assertEquals(listOf("Page two", "Three whole words"), state.contentEdit?.blocks?.get(1)?.map { it.text })
        assertEquals("the line is one run now", listOf(21L), state.contentEdit?.blocks?.get(1)?.last()?.runs?.map { it.id })
    }

    @Test
    fun aRetypedLineIsOneUndoStep() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "Three whole words")
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(listOf("Page two", "Two runs"), state.contentEdit?.blocks?.get(1)?.map { it.text })
        assertFalse("nothing else is queued", state.canUndoAnnotations)
    }

    @Test
    fun aSecondRetypeSendsTheLineAsItNowReads() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "Three whole words")
        advanceUntilIdle()
        viewModel.tapContent(1, onTwo, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "Four")
        advanceUntilIdle()

        // The core folds it into the queued batch when the block carries the first run's id and the text it now shows.
        assertEquals(listOf(listOf(21L, 22L) to "Two runs", listOf(21L) to "Three whole words"), document.retypes.map { (block, _) -> block.runs.map { it.id } to block.text })
    }

    @Test
    fun retypingALineToItsOwnTextQueuesNothing() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "Two runs")
        advanceUntilIdle()

        assertTrue(document.retypes.isEmpty())
        assertFalse(viewModel.state.value.isDirty)
        assertNull(viewModel.state.value.contentEdit?.editor)
    }

    @Test
    fun deletingALineRemovesEveryRunInOneStep() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.deleteTextBlock(viewModel.state.value.documentId)
        advanceUntilIdle()

        assertEquals(listOf(listOf(21L, 22L)), document.textDeletes.map { block -> block.runs.map { it.id } })
        assertEquals(listOf("Page two"), viewModel.state.value.contentEdit?.blocks?.get(1)?.map { it.text })
        viewModel.undoAnnotations()
        advanceUntilIdle()
        assertEquals(listOf("Page two", "Two runs"), viewModel.state.value.contentEdit?.blocks?.get(1)?.map { it.text })
        assertFalse(viewModel.state.value.canUndoAnnotations)
    }

    @Test
    fun movingALineCarriesEveryRunByTheSameOffset() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()

        // The line's top-left corner (10, 112) goes to the tap (50, 180): 40 right, 68 up.
        val (sent, dx, dy) = document.textMoves.single()
        assertEquals(listOf(21L, 22L), sent.runs.map { it.id })
        assertEquals(40.0 to 68.0, dx to dy)
        val state = viewModel.state.value
        assertEquals(TEXT_MOVED, state.status)
        assertNull(state.contentEdit?.movingBlock)
        val moved = state.contentEdit?.blocks?.get(1)?.single { it.text == "Two runs" }
        assertEquals("the outline follows the whole line", AnnotationRect(50.0, 168.0, 72.0, 12.0), moved?.bounds)
        assertEquals(listOf(AnnotationRect(50.0, 168.0, 30.0, 12.0), AnnotationRect(82.0, 168.0, 40.0, 12.0)), moved?.runs?.map { it.bounds })
    }

    @Test
    fun aMovedLineIsOneUndoStep() = runTest {
        val viewModel = armed(RetypableDocument())
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(AnnotationRect(10.0, 100.0, 72.0, 12.0), state.contentEdit?.blocks?.get(1)?.single { it.text == "Two runs" }?.bounds)
        assertFalse(state.canUndoAnnotations)
    }

    @Test
    fun aLineStaysOnItsOwnPageWhileMoving() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        assertTrue(document.textMoves.isEmpty())
        assertEquals(21L, viewModel.state.value.contentEdit?.movingBlock?.id)
        assertEquals(textMovePrompt(1), viewModel.state.value.status)
    }

    @Test
    fun aLineMovedBeforeAndAfterIsSentAsItNowStands() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()
        // Opened again where it now is, and moved on: the core folds the second move into the first.
        viewModel.tapContent(1, AnnotationPoint(60.0, 175.0), reach = 0.0)
        advanceUntilIdle()
        viewModel.armTextMove(viewModel.state.value.documentId)
        viewModel.tapContent(1, AnnotationPoint(20.0, 60.0), reach = 0.0)
        advanceUntilIdle()

        assertEquals(2, document.textMoves.size)
        assertEquals(AnnotationRect(50.0, 168.0, 72.0, 12.0), document.textMoves.last().first.bounds)
    }

    @Test
    fun aLineWhoseFirstRunIsCompositeWarnsAboutTheFont() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.openContentEdit()
        viewModel.tapContent(0, AnnotationPoint(20.0, 125.0), reach = 0.0)
        advanceUntilIdle()

        assertTrue(viewModel.state.value.contentEdit?.editor?.block?.substitutesFont == true)
    }

    @Test
    fun aRefusalToSaveFirstIsSaidOnTheEditor() = runTest {
        val refusal = "This text has an unsaved edit — save the document before editing it again."
        val viewModel = opened(RetypableDocument(refusal = PdfCoreError.Failed(refusal)))
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "Four")
        advanceUntilIdle()

        val editor = viewModel.state.value.contentEdit?.editor
        assertEquals(refusal, editor?.error)
        assertEquals("Four", editor?.text)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun aRefusedMoveOfALineSaysWhy() = runTest {
        val refusal = "This text has an unsaved edit — save the document before editing it again."
        val viewModel = armed(RetypableDocument(refusal = PdfCoreError.Failed(refusal)))
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()

        assertEquals(refusal, viewModel.state.value.status)
        assertNull(viewModel.state.value.contentEdit?.movingBlock)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun anUndoClosesAnOpenEditor() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "Four")
        advanceUntilIdle()
        viewModel.tapContent(1, onTwo, reach = 0.0)
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertNull("the block it held may no longer be as it was", viewModel.state.value.contentEdit?.editor)
    }

    @Test
    fun aTapOnALineWhileAnotherDialogIsOpenReplacesIt() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.openContentEdit()
        viewModel.tapContent(0, AnnotationPoint(20.0, 30.0), reach = 0.0)
        advanceUntilIdle()
        viewModel.tapContent(1, onRuns, reach = 0.0)
        advanceUntilIdle()

        val mode = viewModel.state.value.contentEdit
        assertNull("one dialog at a time", mode?.resizer)
        assertEquals(21L, mode?.editor?.block?.id)
    }

    private suspend fun TestScope.opened(document: PdfDocument): ViewerViewModel = openedWith(document).also {
        it.openContentEdit()
        it.tapContent(1, onRuns, reach = 0.0)
        advanceUntilIdle()
    }

    private suspend fun TestScope.armed(document: PdfDocument): ViewerViewModel = opened(document).also {
        it.armTextMove(it.state.value.documentId)
    }

    private suspend fun TestScope.openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
