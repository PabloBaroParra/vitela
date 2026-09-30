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
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Resizing an image the page itself paints, from Edit content: each resize one undoable edit of the shared log. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelImageResizeTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // RetypableDocument's image fills (10, 20)–(40, 50) on page 0.
    private val onImage = AnnotationPoint(20.0, 30.0)

    @Test
    fun aShownPageHasItsImagesReadForTheOutlines() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.openContentEdit()
        viewModel.contentPageShown(0)
        advanceUntilIdle()

        assertEquals(listOf(30L), viewModel.state.value.contentEdit?.images?.get(0)?.map { it.id })
    }

    @Test
    fun aTapOnAnImageOpensItsResizer() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()

        val mode = viewModel.state.value.contentEdit
        assertEquals(30L, mode?.resizer?.image?.id)
        assertEquals("30" to "30", mode?.resizer?.width to mode?.resizer?.height)
        assertNull("one dialog at a time", mode?.editor)
    }

    @Test
    fun aResizeReachesTheCoreKeepingTheTopLeftCorner() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.resizeImage(viewModel.state.value.documentId, "60", "15")
        advanceUntilIdle()

        // Top edge at y = 50 before and after: it grows right and shrinks up from the bottom.
        assertEquals(listOf(30L to AnnotationRect(10.0, 35.0, 60.0, 15.0)), document.resizes.map { (image, to) -> image.id to to })
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(IMAGE_RESIZED, state.status)
        assertNull(state.contentEdit?.resizer)
        assertEquals("the outline must describe the box now filled", AnnotationRect(10.0, 35.0, 60.0, 15.0), state.contentEdit?.images?.get(0)?.single()?.bounds)
    }

    @Test
    fun aResizeRedrawsThePage() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        advanceUntilIdle()
        val renders = document.drawn.size
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.resizeImage(viewModel.state.value.documentId, "60", "15")
        advanceUntilIdle()

        assertTrue("only the renderer can paint the resized image", document.drawn.drop(renders).any { it.first == 0 })
    }

    @Test
    fun aDoubleTapOnResizeQueuesOneResize() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.resizeImage(viewModel.state.value.documentId, "60", "15")
        viewModel.resizeImage(viewModel.state.value.documentId, "60", "15")
        advanceUntilIdle()

        assertEquals("the dialog the second tap came from was already answered", 1, document.resizes.size)
    }

    @Test
    fun aSizeThatIsNotAPositiveNumberKeepsTheResizerOpenWithTheReason() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.resizeImage(viewModel.state.value.documentId, "wide", "0")
        advanceUntilIdle()

        val resizer = viewModel.state.value.contentEdit?.resizer
        assertNotNull(resizer)
        assertEquals(IMAGE_SIZE_INVALID, resizer!!.error)
        assertEquals("what was typed stays, to be fixed", "wide" to "0", resizer.width to resizer.height)
        assertTrue(document.resizes.isEmpty())
    }

    @Test
    fun aCommaIsADecimalPoint() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.resizeImage(viewModel.state.value.documentId, "30,5", "30")
        advanceUntilIdle()

        assertEquals(30.5, document.resizes.single().second.width, 0.0)
    }

    @Test
    fun anUnchangedSizeQueuesNothing() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.resizeImage(viewModel.state.value.documentId, "30", "30.0")
        advanceUntilIdle()

        assertTrue(document.resizes.isEmpty())
        assertFalse(viewModel.state.value.isDirty)
        assertNull(viewModel.state.value.contentEdit?.resizer)
    }

    @Test
    fun aRefusedResizeKeepsTheResizerOpenWithTheReason() = runTest {
        val refusal = "This image cannot be resized."
        val viewModel = openedWith(RetypableDocument(refusal = PdfCoreError.Failed(refusal)))
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.resizeImage(viewModel.state.value.documentId, "60", "15")
        advanceUntilIdle()

        val resizer = viewModel.state.value.contentEdit?.resizer
        assertEquals(refusal, resizer?.error)
        assertEquals("60" to "15", resizer?.width to resizer?.height)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun undoingAResizeRereadsTheImages() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.resizeImage(viewModel.state.value.documentId, "60", "15")
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(AnnotationRect(10.0, 20.0, 30.0, 30.0), viewModel.state.value.contentEdit?.images?.get(0)?.single()?.bounds)
    }

    @Test
    fun aResizeForAReplacedDocumentIsDropped() = runTest {
        val first = RetypableDocument()
        val second = RetypableDocument()
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(first, second))
        viewModel.open("a.pdf", byteArrayOf(1))
        advanceUntilIdle()
        val staleId = viewModel.state.value.documentId
        viewModel.open("b.pdf", byteArrayOf(2))
        advanceUntilIdle()
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.resizeImage(staleId, "60", "15")
        advanceUntilIdle()

        assertTrue(first.resizes.isEmpty())
        assertTrue(second.resizes.isEmpty())
    }

    @Test
    fun dismissingTheResizerQueuesNothing() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.dismissImageResizer()

        assertNull(viewModel.state.value.contentEdit?.resizer)
        assertTrue(document.resizes.isEmpty())
    }

    private suspend fun TestScope.openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
