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
 * Selecting, reshaping, moving and deleting a vector path the page itself
 * paints, from Edit content: a tap opens the shape dialog, each action is one
 * undoable edit.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelPathTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // RetypableDocument's path 40 is a hairline rule from (10, 80) to (70, 80) on page 0;
    // path 41 is a filled box (60, 90)–(90, 110). Nothing is painted at (50, 180).
    private val onRule = AnnotationPoint(30.0, 80.0)
    private val onBox = AnnotationPoint(75.0, 100.0)
    private val clear = AnnotationPoint(50.0, 180.0)

    @Test
    fun aTapOnAHairlineRuleOpensTheShapeDialog() = runTest {
        val viewModel = opened(RetypableDocument(), onRule)

        val mode = viewModel.state.value.contentEdit
        assertEquals(40L, mode?.shaper?.path?.id)
        assertEquals("a flat axis reads as zero", "0", mode?.shaper?.height)
        assertNull(mode?.editor)
        assertNull(mode?.resizer)
    }

    @Test
    fun theOutlinesComeFromTheSameRead() = runTest {
        val viewModel = opened(RetypableDocument(), onBox)

        assertEquals(listOf(40L, 41L), viewModel.state.value.contentEdit?.paths?.get(0)?.map { it.id })
    }

    @Test
    fun resizeKeepsTheTopLeftCornerAndRedraws() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document, onBox)
        viewModel.resizePath(viewModel.state.value.documentId, "60", "10")
        advanceUntilIdle()

        assertEquals(listOf(41L to AnnotationRect(60.0, 100.0, 60.0, 10.0)), document.pathResizes.map { (path, to) -> path.id to to })
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(PATH_RESIZED, state.status)
        assertNull(state.contentEdit?.shaper)
        assertEquals(AnnotationRect(60.0, 100.0, 60.0, 10.0), state.contentEdit?.paths?.get(0)?.first { it.id == 41L }?.bounds)
    }

    @Test
    fun aFlatRuleResizesOnlyItsLength() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document, onRule)
        viewModel.resizePath(viewModel.state.value.documentId, "90", "12")
        advanceUntilIdle()

        assertEquals(AnnotationRect(10.0, 80.0, 90.0, 0.0), document.pathResizes.single().second)
    }

    @Test
    fun aSizeThatIsNotValidKeepsTheDialogOpenWithWhatWasTyped() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document, onBox)
        viewModel.resizePath(viewModel.state.value.documentId, "0", "abc")
        advanceUntilIdle()

        val shaper = viewModel.state.value.contentEdit?.shaper
        assertEquals(PATH_SIZE_INVALID, shaper?.error)
        assertEquals("0", shaper?.width)
        assertEquals("abc", shaper?.height)
        assertTrue(document.pathResizes.isEmpty())
    }

    @Test
    fun resizingToTheSameSizeQueuesNothing() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document, onBox)
        viewModel.resizePath(viewModel.state.value.documentId, "30", "20")
        advanceUntilIdle()

        assertTrue(document.pathResizes.isEmpty())
        assertNull(viewModel.state.value.contentEdit?.shaper)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun aRefusedResizeKeepsTheDialogAndSaysWhy() = runTest {
        val refusal = "This shape cannot be resized."
        val viewModel = opened(RetypableDocument(refusal = PdfCoreError.Failed(refusal)), onBox)
        viewModel.resizePath(viewModel.state.value.documentId, "60", "10")
        advanceUntilIdle()

        val shaper = viewModel.state.value.contentEdit?.shaper
        assertEquals(refusal, shaper?.error)
        assertEquals("60", shaper?.width)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun moveClosesTheDialogAndArmsATap() = runTest {
        val viewModel = armed(RetypableDocument(), onRule)

        val state = viewModel.state.value
        assertNull(state.contentEdit?.shaper)
        assertEquals(40L, state.contentEdit?.movingPath?.id)
        assertEquals(pathMovePrompt(0), state.status)
    }

    @Test
    fun theArmedTapTakesTheTopLeftCornerKeepingTheSize() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document, onBox)
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        assertEquals(listOf(41L to AnnotationRect(50.0, 160.0, 30.0, 20.0)), document.pathMoves.map { (path, to) -> path.id to to })
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertEquals(PATH_MOVED, state.status)
        assertNull("one tap, one move", state.contentEdit?.movingPath)
        assertEquals(AnnotationRect(50.0, 160.0, 30.0, 20.0), state.contentEdit?.paths?.get(0)?.first { it.id == 41L }?.bounds)
    }

    @Test
    fun aFlatRuleMovesWithItsCornerOnTheTap() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document, onRule)
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        assertEquals(AnnotationRect(50.0, 180.0, 60.0, 0.0), document.pathMoves.single().second)
    }

    @Test
    fun aTapOnAnotherPageMovesNothingAndKeepsTheMoveArmed() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document, onBox)
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()

        assertTrue(document.pathMoves.isEmpty())
        assertEquals(41L, viewModel.state.value.contentEdit?.movingPath?.id)
        assertEquals(pathMovePrompt(0), viewModel.state.value.status)
    }

    @Test
    fun aMoveToWhereItIsSaysSo() = runTest {
        val document = RetypableDocument()
        val viewModel = armed(document, onBox)
        // The box's top-left corner is (60, 110).
        viewModel.tapContent(0, AnnotationPoint(60.0, 110.0), reach = 0.0)
        advanceUntilIdle()

        assertTrue(document.pathMoves.isEmpty())
        assertEquals(PATH_POSITION_UNCHANGED, viewModel.state.value.status)
        assertNull(viewModel.state.value.contentEdit?.movingPath)
    }

    @Test
    fun cancellingTheMoveDisarmsIt() = runTest {
        val viewModel = armed(RetypableDocument(), onBox)
        viewModel.cancelContentMove()

        assertNull(viewModel.state.value.contentEdit?.moving)
        assertEquals(MOVE_CANCELLED, viewModel.state.value.status)
    }

    @Test
    fun deleteRemovesThePathAndItsOutline() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document, onRule)
        viewModel.deletePath(viewModel.state.value.documentId)
        advanceUntilIdle()

        assertEquals(listOf(40L), document.pathDeletes.map { it.id })
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(PATH_DELETED, state.status)
        assertNull(state.contentEdit?.shaper)
        assertEquals(listOf(41L), state.contentEdit?.paths?.get(0)?.map { it.id })
    }

    @Test
    fun aDoubleTapOnDeleteQueuesOneEdit() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document, onRule)
        viewModel.deletePath(viewModel.state.value.documentId)
        viewModel.deletePath(viewModel.state.value.documentId)
        advanceUntilIdle()

        assertEquals(1, document.pathDeletes.size)
    }

    @Test
    fun aRefusedDeleteSaysWhyAndKeepsThePath() = runTest {
        val refusal = "This shape cannot be deleted."
        val viewModel = opened(RetypableDocument(refusal = PdfCoreError.Failed(refusal)), onRule)
        viewModel.deletePath(viewModel.state.value.documentId)
        advanceUntilIdle()

        assertEquals(refusal, viewModel.state.value.status)
        assertEquals(listOf(40L, 41L), viewModel.state.value.contentEdit?.paths?.get(0)?.map { it.id })
    }

    @Test
    fun undoingADeleteBringsTheOutlineBackAndDropsTheDialog() = runTest {
        val viewModel = opened(RetypableDocument(), onRule)
        viewModel.deletePath(viewModel.state.value.documentId)
        advanceUntilIdle()
        viewModel.tapContent(0, onBox, reach = 0.0)
        advanceUntilIdle()
        assertEquals(41L, viewModel.state.value.contentEdit?.shaper?.path?.id)
        viewModel.undoAnnotations()
        advanceUntilIdle()

        val mode = viewModel.state.value.contentEdit
        assertNull("an undo may have changed the path the dialog holds", mode?.shaper)
        assertEquals(listOf(40L, 41L), mode?.paths?.get(0)?.map { it.id })
    }

    @Test
    fun anUndoWhileAMoveIsArmedDisarmsIt() = runTest {
        val viewModel = armed(RetypableDocument(), onBox)
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()
        // Armed again on the box where it now is — which the undo takes back.
        viewModel.tapContent(0, AnnotationPoint(55.0, 170.0), reach = 0.0)
        advanceUntilIdle()
        viewModel.armPathMove(viewModel.state.value.documentId)
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertNull("the path it held may no longer be where it was", viewModel.state.value.contentEdit?.movingPath)
    }

    @Test
    fun actionsFromADialogBuiltForAReplacedDocumentDoNothing() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document, onRule)
        val stale = viewModel.state.value.documentId + 1
        viewModel.resizePath(stale, "90", "0")
        viewModel.deletePath(stale)
        viewModel.armPathMove(stale)
        advanceUntilIdle()

        assertTrue(document.pathResizes.isEmpty() && document.pathDeletes.isEmpty())
        assertEquals(40L, viewModel.state.value.contentEdit?.shaper?.path?.id)
        assertNull(viewModel.state.value.contentEdit?.moving)
    }

    @Test
    fun dismissingTheDialogLeavesThePathAlone() = runTest {
        val viewModel = opened(RetypableDocument(), onRule)
        viewModel.dismissPathShaper()

        assertNull(viewModel.state.value.contentEdit?.shaper)
    }

    /** Open, with Edit content armed and the shape dialog open on the path at [at] on page 0. */
    private suspend fun TestScope.opened(document: PdfDocument, at: AnnotationPoint): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        viewModel.openContentEdit()
        viewModel.tapContent(0, at, reach = 0.0)
        advanceUntilIdle()
        return viewModel
    }

    /** [opened], then the dialog's Move. */
    private suspend fun TestScope.armed(document: PdfDocument, at: AnnotationPoint): ViewerViewModel {
        val viewModel = opened(document, at)
        viewModel.armPathMove(viewModel.state.value.documentId)
        return viewModel
    }
}
