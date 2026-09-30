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

/** Moving an image the page itself paints, from Edit content: Move arms a tap, the tap is one undoable edit. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelImageMoveTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // RetypableDocument's image fills (10, 20)–(40, 50) on page 0; nothing is painted at (50, 180).
    private val onImage = AnnotationPoint(20.0, 30.0)
    private val clear = AnnotationPoint(50.0, 180.0)

    @Test
    fun moveClosesTheDialogAndArmsATap() = runTest {
        val viewModel = armed(RetypableDocument())

        val state = viewModel.state.value
        assertNull(state.contentEdit?.resizer)
        assertEquals(30L, state.contentEdit?.movingImage?.id)
        assertEquals(imageMovePrompt(0), state.status)
    }

    @Test
    fun theArmedTapTakesTheImagesTopLeftCornerKeepingItsSize() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        assertEquals(listOf(30L to AnnotationRect(50.0, 150.0, 30.0, 30.0)), document.moves.map { (image, to) -> image.id to to })
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(IMAGE_MOVED, state.status)
        assertNull("one tap, one move", state.contentEdit?.movingImage)
        assertEquals("the outline must describe where the image now is", AnnotationRect(50.0, 150.0, 30.0, 30.0), state.contentEdit?.images?.get(0)?.single()?.bounds)
    }

    @Test
    fun aMoveIsNotKeptOnThePage() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        // Page 0 is 100 pt wide: the image hangs 10 pt off its right edge, as sent.
        viewModel.tapContent(0, AnnotationPoint(80.0, 180.0), reach = 0.0)
        advanceUntilIdle()

        assertEquals(AnnotationRect(80.0, 150.0, 30.0, 30.0), document.moves.single().second)
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

        assertTrue("only the renderer can paint the moved image", document.drawn.drop(renders).any { it.first == 0 })
    }

    @Test
    fun aTapOnAnotherPageMovesNothingAndStaysArmed() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()

        assertTrue(document.moves.isEmpty())
        assertEquals(30L, viewModel.state.value.contentEdit?.movingImage?.id)
        assertEquals(imageMovePrompt(0), viewModel.state.value.status)
    }

    @Test
    fun aDoubleTapQueuesOneMove() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.tapContent(0, clear, reach = 0.0)
        viewModel.tapContent(0, AnnotationPoint(60.0, 190.0), reach = 0.0)
        advanceUntilIdle()

        assertEquals("the second tap found the move already spent", 1, document.moves.size)
    }

    @Test
    fun aTapOnTheCornerItAlreadyHasQueuesNothing() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.tapContent(0, AnnotationPoint(10.0, 50.0), reach = 0.0)
        advanceUntilIdle()

        assertTrue(document.moves.isEmpty())
        assertFalse(viewModel.state.value.isDirty)
        assertNull(viewModel.state.value.contentEdit?.movingImage)
    }

    @Test
    fun aRefusedMoveSaysWhyAndDisarms() = runTest {
        val refusal = "This image cannot be moved."
        val viewModel = armed(RetypableDocument(refusal = PdfCoreError.Failed(refusal)))
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(refusal, state.status)
        assertNull(state.contentEdit?.movingImage)
        assertFalse(state.isDirty)
    }

    @Test
    fun undoingAMoveRereadsTheImages() = runTest {
        val viewModel = armed(RetypableDocument())
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(AnnotationRect(10.0, 20.0, 30.0, 30.0), viewModel.state.value.contentEdit?.images?.get(0)?.single()?.bounds)
    }

    @Test
    fun anUndoDisarmsAPendingMove() = runTest {
        val viewModel = armed(RetypableDocument())
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()
        // Armed again on the image where it now is — which the undo takes back.
        viewModel.tapContent(0, AnnotationPoint(60.0, 160.0), reach = 0.0)
        advanceUntilIdle()
        viewModel.armImageMove(viewModel.state.value.documentId)
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertNull("the image it held may no longer be where it was", viewModel.state.value.contentEdit?.movingImage)
    }

    @Test
    fun closingTheModeDisarmsAMove() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.closeContentEdit()
        viewModel.openContentEdit()
        advanceUntilIdle()
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        assertTrue(document.moves.isEmpty())
    }

    @Test
    fun cancellingAMoveLeavesTheNextTapToOpenWhatItHits() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document)
        viewModel.cancelImageMove()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()

        assertTrue(document.moves.isEmpty())
        assertEquals(30L, viewModel.state.value.contentEdit?.resizer?.image?.id)
    }

    @Test
    fun moveFromADialogBuiltForAReplacedDocumentArmsNothing() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.armImageMove(viewModel.state.value.documentId + 1)

        assertNull(viewModel.state.value.contentEdit?.movingImage)
    }

    private suspend fun TestScope.armed(document: PdfDocument): ViewerViewModel = openedWith(document).also { arm(it) }

    private fun TestScope.arm(viewModel: ViewerViewModel) {
        viewModel.openContentEdit()
        viewModel.tapContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.armImageMove(viewModel.state.value.documentId)
    }

    private suspend fun TestScope.openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
