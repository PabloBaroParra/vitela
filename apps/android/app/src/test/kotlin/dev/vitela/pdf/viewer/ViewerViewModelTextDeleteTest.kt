package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
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

/** Deleting a line of text the page itself paints, from the retype dialog: one undoable edit. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelTextDeleteTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // RetypableDocument's "Hello" fills (10, 150)–(50, 162) on page 0.
    private val onHello = AnnotationPoint(20.0, 155.0)

    @Test
    fun deleteRemovesTheRunAndItsOutline() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.deleteTextRun(viewModel.state.value.documentId)
        advanceUntilIdle()

        assertEquals(listOf(10L), document.textDeletes.map { it.id })
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(TEXT_DELETED, state.status)
        assertNull(state.contentEdit?.editor)
        assertEquals("the outline must go with the run", listOf(11L), state.contentEdit?.runs?.get(0)?.map { it.id })
    }

    @Test
    fun aDeleteRedrawsThePageWithoutTheRun() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        val renders = document.drawn.size
        viewModel.deleteTextRun(viewModel.state.value.documentId)
        advanceUntilIdle()

        val redrawn = document.drawn.drop(renders).last { it.first == 0 }
        assertEquals("only the renderer can take the words off the page", listOf("World"), redrawn.second)
    }

    @Test
    fun aDoubleTapOnDeleteQueuesOneEdit() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.deleteTextRun(viewModel.state.value.documentId)
        viewModel.deleteTextRun(viewModel.state.value.documentId)
        advanceUntilIdle()

        assertEquals("the second tap found the dialog already spent", 1, document.textDeletes.size)
    }

    @Test
    fun aRefusedDeleteSaysWhyAndKeepsTheRun() = runTest {
        val refusal = "This text cannot be deleted."
        val viewModel = opened(RetypableDocument(refusal = PdfCoreError.Failed(refusal)))
        viewModel.deleteTextRun(viewModel.state.value.documentId)
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(refusal, state.status)
        assertFalse(state.isDirty)
        assertEquals(listOf(10L, 11L), state.contentEdit?.runs?.get(0)?.map { it.id })
    }

    @Test
    fun undoingADeleteBringsTheOutlineBack() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.deleteTextRun(viewModel.state.value.documentId)
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(listOf(10L, 11L), viewModel.state.value.contentEdit?.runs?.get(0)?.map { it.id })
    }

    @Test
    fun deleteFromADialogBuiltForAReplacedDocumentDeletesNothing() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.deleteTextRun(viewModel.state.value.documentId + 1)
        advanceUntilIdle()

        assertTrue(document.textDeletes.isEmpty())
        assertEquals(10L, viewModel.state.value.contentEdit?.editor?.run?.id)
    }

    /** Open, with Edit content armed and the retype dialog open on page 0's "Hello". */
    private suspend fun TestScope.opened(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        viewModel.openContentEdit()
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        return viewModel
    }
}
