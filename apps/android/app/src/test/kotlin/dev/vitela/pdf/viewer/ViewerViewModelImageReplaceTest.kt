package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Replacing the source of an image the page itself paints: the core checks it
 * can recover the original before a picker opens, then the chosen file fills
 * the same box as one undoable edit.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelImageReplaceTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // RetypableDocument's image fills (10, 20)–(40, 50) on page 0.
    private val onImage = AnnotationPoint(20.0, 30.0)
    private val png = byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47)

    @Test
    fun preparingChecksTheOriginalAndClosesTheDialogForThePicker() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)

        assertTrue(viewModel.prepareImageReplacement(viewModel.state.value.documentId))

        assertEquals(listOf(30L), document.prepared.map { it.id })
        val mode = viewModel.state.value.contentEdit
        assertNull(mode?.resizer)
        assertEquals(30L, mode?.replacingImage?.id)
    }

    @Test
    fun aRefusedPreparationSaysWhyAndOpensNoPicker() = runTest {
        val refusal = "Save before replacing an image with a pending edit."
        val viewModel = opened(RetypableDocument(refusal = PdfCoreError.Failed(refusal)))

        assertFalse(viewModel.prepareImageReplacement(viewModel.state.value.documentId))

        val state = viewModel.state.value
        assertEquals(refusal, state.status)
        assertNull(state.contentEdit?.resizer)
        assertNull(state.contentEdit?.replacingImage)
    }

    @Test
    fun preparingFromADialogBuiltForAReplacedDocumentOpensNoPicker() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)

        assertFalse(viewModel.prepareImageReplacement(viewModel.state.value.documentId + 1))

        assertTrue(document.prepared.isEmpty())
        assertEquals(30L, viewModel.state.value.contentEdit?.resizer?.image?.id)
    }

    @Test
    fun thePickedFileReplacesTheImageAsOneUndoableEdit() = runTest {
        val document = RetypableDocument()
        val viewModel = prepared(document)
        val renders = document.drawn.size
        viewModel.replaceImage(png)
        advanceUntilIdle()

        val (image, bytes) = document.replaces.single()
        assertEquals(30L, image.id)
        assertArrayEquals(png, bytes)
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(IMAGE_REPLACED, state.status)
        assertNull(state.contentEdit?.replacingImage)
        assertTrue("only the renderer can show the new picture", document.drawn.drop(renders).any { it.first == 0 })
    }

    @Test
    fun aSecondPickBeforeTheAnswerQueuesOneEdit() = runTest {
        val document = RetypableDocument()
        val viewModel = prepared(document)
        viewModel.replaceImage(png)
        viewModel.replaceImage(png)
        advanceUntilIdle()

        assertEquals(1, document.replaces.size)
    }

    @Test
    fun aRefusedReplacementSaysWhyAndLeavesTheDocumentClean() = runTest {
        val refusal = "The chosen file is not a PNG or JPEG image."
        val document = object : PdfDocument by RetypableDocument() {
            override fun replaceImage(image: ContentImage, imageBytes: ByteArray) =
                PdfCoreResult.Failure(PdfCoreError.Failed(refusal))
        }
        val viewModel = prepared(document)
        viewModel.replaceImage(png)
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(refusal, state.status)
        assertFalse(state.isDirty)
        assertNull(state.contentEdit?.replacingImage)
    }

    @Test
    fun dismissingThePickerReplacesNothing() = runTest {
        val document = RetypableDocument()
        val viewModel = prepared(document)
        viewModel.cancelImageReplacement(IMAGE_REPLACE_CANCELLED)
        viewModel.replaceImage(png)
        advanceUntilIdle()

        assertTrue(document.replaces.isEmpty())
        assertEquals(IMAGE_REPLACE_CANCELLED, viewModel.state.value.status)
        assertNull(viewModel.state.value.contentEdit?.replacingImage)
    }

    @Test
    fun anUndoWhileThePickerIsOpenDropsTheReplacement() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        // Something to undo, so the undo re-reads the pages.
        viewModel.resizeImage(viewModel.state.value.documentId, "20", "20")
        advanceUntilIdle()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        assertTrue(viewModel.prepareImageReplacement(viewModel.state.value.documentId))
        viewModel.undoAnnotations()
        advanceUntilIdle()
        viewModel.replaceImage(png)
        advanceUntilIdle()

        assertTrue("the image the picker was opened for may be gone", document.replaces.isEmpty())
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

    /** [opened], with the original checked and the picker about to open. */
    private suspend fun TestScope.prepared(document: PdfDocument): ViewerViewModel =
        opened(document).also { viewModel -> assertTrue(viewModel.prepareImageReplacement(viewModel.state.value.documentId)) }
}
