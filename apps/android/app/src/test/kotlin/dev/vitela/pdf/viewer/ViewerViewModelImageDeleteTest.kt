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

/** Deleting an image the page itself paints, from the image dialog: one undoable edit. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelImageDeleteTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // RetypableDocument's image fills (10, 20)–(40, 50) on page 0.
    private val onImage = AnnotationPoint(20.0, 30.0)

    @Test
    fun deleteRemovesTheImageAndItsOutline() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.deleteImage(viewModel.state.value.documentId)
        advanceUntilIdle()

        assertEquals(listOf(30L), document.deletes.map { it.id })
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(IMAGE_DELETED, state.status)
        assertNull(state.contentEdit?.resizer)
        assertEquals("the outline must go with the image", emptyList<Any>(), state.contentEdit?.images?.get(0))
    }

    @Test
    fun aDeleteRedrawsThePage() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        val renders = document.drawn.size
        viewModel.deleteImage(viewModel.state.value.documentId)
        advanceUntilIdle()

        assertTrue("only the renderer can take the image off the page", document.drawn.drop(renders).any { it.first == 0 })
    }

    @Test
    fun aDoubleTapOnDeleteQueuesOneEdit() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.deleteImage(viewModel.state.value.documentId)
        viewModel.deleteImage(viewModel.state.value.documentId)
        advanceUntilIdle()

        assertEquals("the second tap found the dialog already spent", 1, document.deletes.size)
    }

    @Test
    fun aRefusedDeleteSaysWhyAndKeepsTheImage() = runTest {
        val refusal = "This image cannot be deleted."
        val viewModel = opened(RetypableDocument(refusal = PdfCoreError.Failed(refusal)))
        viewModel.deleteImage(viewModel.state.value.documentId)
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(refusal, state.status)
        assertFalse(state.isDirty)
        assertEquals(30L, state.contentEdit?.images?.get(0)?.single()?.id)
    }

    @Test
    fun undoingADeleteBringsTheOutlineBack() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.deleteImage(viewModel.state.value.documentId)
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(AnnotationRect(10.0, 20.0, 30.0, 30.0), viewModel.state.value.contentEdit?.images?.get(0)?.single()?.bounds)
    }

    @Test
    fun deleteFromADialogBuiltForAReplacedDocumentDeletesNothing() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.deleteImage(viewModel.state.value.documentId + 1)
        advanceUntilIdle()

        assertTrue(document.deletes.isEmpty())
        assertEquals(30L, viewModel.state.value.contentEdit?.resizer?.image?.id)
    }

    /** Open, with Edit content armed and the image dialog open on page 0's image. */
    private suspend fun TestScope.opened(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        return viewModel
    }
}
