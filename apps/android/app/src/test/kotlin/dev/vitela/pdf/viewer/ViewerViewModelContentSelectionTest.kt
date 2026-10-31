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
 * Selecting several items on one page — a marquee, a long-press, taps that
 * add and remove — then moving or deleting them as one undo step (B25 T-208).
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelContentSelectionTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // RetypableDocument, page 0: blocks "Hello" (10, 150) and "World" (10, 120), image 30 at (10, 20)–(40, 50),
    // rule 40 from (10, 80) to (70, 80), box 41 (60, 90)–(90, 110). Page 1: "Page two" and the two-run block "Two runs".
    private val wholePage = AnnotationRect(0.0, 0.0, 100.0, 200.0)
    private val onHello = AnnotationPoint(20.0, 156.0)
    private val onImage = AnnotationPoint(20.0, 30.0)
    private val onRule = AnnotationPoint(30.0, 80.0)
    private val clear = AnnotationPoint(50.0, 180.0)

    private fun ViewerViewModel.selection() = state.value.contentEdit?.selection

    @Test
    fun aMarqueeSelectsEverythingItTouchesOfEveryKind() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.marqueeContent(0, wholePage)
        advanceUntilIdle()

        val selection = viewModel.selection()
        assertEquals(0, selection?.pageIndex)
        assertEquals(listOf(10L, 11L), selection?.blocks?.map { it.id })
        assertEquals(listOf(30L), selection?.images?.map { it.id })
        assertEquals(listOf(40L, 41L), selection?.paths?.map { it.id })
        assertEquals(selectedMessage(5), viewModel.state.value.status)
    }

    @Test
    fun aMarqueeThatOnlyCrossesAHairlineRuleStillSelectsIt() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.marqueeContent(0, AnnotationRect(20.0, 79.0, 10.0, 2.0))
        advanceUntilIdle()

        assertEquals(listOf(40L), viewModel.selection()?.paths?.map { it.id })
        assertEquals(1, viewModel.selection()?.count)
    }

    @Test
    fun aMarqueeOverNothingSelectsNothingAndSaysSo() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.marqueeContent(0, AnnotationRect(45.0, 175.0, 10.0, 10.0))
        advanceUntilIdle()

        assertNull(viewModel.selection())
        assertEquals(SELECTION_EMPTY, viewModel.state.value.status)
    }

    @Test
    fun aMarqueeOnATwoRunBlockSelectsTheBlockWithBothRuns() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.marqueeContent(1, AnnotationRect(0.0, 95.0, 100.0, 20.0))
        advanceUntilIdle()

        assertEquals(listOf(listOf(21L, 22L)), viewModel.selection()?.blocks?.map { block -> block.runs.map { it.id } })
    }

    @Test
    fun aLongPressOnAnItemStartsASelectionWithIt() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.pressContent(0, onImage, reach = 0.0)
        advanceUntilIdle()

        assertEquals(listOf(30L), viewModel.selection()?.images?.map { it.id })
        assertEquals(selectedMessage(1), viewModel.state.value.status)
        assertNull("a press selects, it does not open the dialog", viewModel.state.value.contentEdit?.resizer)
    }

    @Test
    fun aLongPressOnNothingClearsTheSelectionAndSaysSo() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.pressContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.pressContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        assertNull(viewModel.selection())
        assertEquals(SELECTION_EMPTY, viewModel.state.value.status)
    }

    @Test
    fun withASelectionATapOnAnItemAddsItAndAgainTakesItOut() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.pressContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()

        assertEquals(listOf(10L), viewModel.selection()?.blocks?.map { it.id })
        assertEquals(listOf(30L), viewModel.selection()?.images?.map { it.id })
        assertEquals(selectedMessage(2), viewModel.state.value.status)
        assertNull("no dialog opens over a selection", viewModel.state.value.contentEdit?.editor)

        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()

        assertEquals(emptyList<Long>(), viewModel.selection()?.blocks?.map { it.id })
        assertEquals(listOf(30L), viewModel.selection()?.images?.map { it.id })
    }

    @Test
    fun takingOutTheLastItemEndsTheSelection() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.pressContent(0, onRule, reach = 0.0)
        advanceUntilIdle()
        viewModel.tapContent(0, onRule, reach = 0.0)
        advanceUntilIdle()

        assertNull(viewModel.selection())
    }

    @Test
    fun aTapOnEmptyPageClearsTheSelection() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.marqueeContent(0, wholePage)
        advanceUntilIdle()
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        assertNull(viewModel.selection())
        assertEquals(SELECTION_CLEARED, viewModel.state.value.status)
    }

    @Test
    fun aTapOnAnotherPageClearsTheSelectionAndOpensNothing() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.pressContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.tapContent(1, AnnotationPoint(20.0, 156.0), reach = 0.0)
        advanceUntilIdle()

        assertNull(viewModel.selection())
        assertNull(viewModel.state.value.contentEdit?.editor)
    }

    @Test
    fun withNoSelectionATapStillOpensTheDialog() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()

        assertEquals(10L, viewModel.state.value.contentEdit?.editor?.block?.id)
        assertNull(viewModel.selection())
    }

    @Test
    fun dragTheSelectionSendsOneMoveOfEveryMember() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.marqueeContent(0, wholePage)
        advanceUntilIdle()
        viewModel.moveContentSelection(0, dx = 5.0, dy = -10.0)
        advanceUntilIdle()

        val (sent, dx, dy) = document.selectionMoves.single()
        assertEquals(5, sent.count)
        assertEquals(5.0, dx, 0.0)
        assertEquals(-10.0, dy, 0.0)
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(SELECTION_MOVED, state.status)
    }

    @Test
    fun theMovedSelectionStaysSelectedWhereItLanded() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.pressContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.moveContentSelection(0, dx = 5.0, dy = -10.0)
        advanceUntilIdle()

        val image = viewModel.selection()?.images?.single()
        assertEquals(AnnotationRect(15.0, 10.0, 30.0, 30.0), image?.bounds)
        assertEquals("the outlines follow the same read", image, viewModel.state.value.contentEdit?.images?.get(0)?.single())
    }

    @Test
    fun aSecondDragMovesTheMembersAsTheyNowAre() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.pressContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.moveContentSelection(0, dx = 5.0, dy = 0.0)
        advanceUntilIdle()
        viewModel.moveContentSelection(0, dx = 0.0, dy = 7.0)
        advanceUntilIdle()

        assertEquals(AnnotationRect(15.0, 20.0, 30.0, 30.0), document.selectionMoves.last().first.images.single().bounds)
        assertEquals(AnnotationRect(15.0, 27.0, 30.0, 30.0), viewModel.selection()?.images?.single()?.bounds)
    }

    @Test
    fun oneUndoTakesTheWholeMoveBackAndDropsTheSelection() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.marqueeContent(0, wholePage)
        advanceUntilIdle()
        viewModel.moveContentSelection(0, dx = 5.0, dy = -10.0)
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertNull("its members may be stale after an undo", viewModel.selection())
        assertEquals(AnnotationRect(10.0, 20.0, 30.0, 30.0), viewModel.state.value.contentEdit?.images?.get(0)?.single()?.bounds)
        assertFalse(viewModel.state.value.canUndoAnnotations)
    }

    @Test
    fun aDragThatWentNowhereQueuesNothing() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.pressContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.moveContentSelection(0, dx = 0.0, dy = 0.0)
        advanceUntilIdle()

        assertTrue(document.selectionMoves.isEmpty())
        assertFalse(viewModel.state.value.isDirty)
        assertEquals(SELECTION_POSITION_UNCHANGED, viewModel.state.value.status)
        assertTrue(viewModel.selection() != null)
    }

    @Test
    fun aRefusedMoveSaysWhyAndKeepsTheSelection() = runTest {
        val reason = "This text cannot be moved."
        val document = RetypableDocument(refusal = PdfCoreError.Failed(reason))
        val viewModel = opened(document)
        viewModel.pressContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.moveContentSelection(0, dx = 5.0, dy = 0.0)
        advanceUntilIdle()

        assertEquals(reason, viewModel.state.value.status)
        assertFalse(viewModel.state.value.isDirty)
        assertEquals(listOf(30L), viewModel.selection()?.images?.map { it.id })
    }

    @Test
    fun deleteRemovesEveryMemberInOneStepAndEndsTheSelection() = runTest {
        val document = RetypableDocument()
        val viewModel = opened(document)
        viewModel.marqueeContent(0, wholePage)
        advanceUntilIdle()
        viewModel.deleteContentSelection()
        advanceUntilIdle()

        assertEquals(5, document.selectionDeletes.single().count)
        val state = viewModel.state.value
        assertNull(state.contentEdit?.selection)
        assertTrue(state.isDirty)
        assertEquals(SELECTION_DELETED, state.status)
        assertTrue(state.contentEdit?.blocks?.get(0)?.isEmpty() == true)
        assertTrue(state.contentEdit?.images?.get(0)?.isEmpty() == true)
        assertTrue(state.contentEdit?.paths?.get(0)?.isEmpty() == true)

        viewModel.undoAnnotations()
        advanceUntilIdle()
        assertEquals(5, (viewModel.state.value.contentEdit?.let { it.blocks[0].orEmpty().size + it.images[0].orEmpty().size + it.paths[0].orEmpty().size }))
    }

    @Test
    fun aRefusedDeleteSaysWhyAndKeepsTheSelection() = runTest {
        val reason = "This document does not permit content changes."
        val viewModel = opened(RetypableDocument(refusal = PdfCoreError.Failed(reason)))
        viewModel.pressContent(0, onImage, reach = 0.0)
        advanceUntilIdle()
        viewModel.deleteContentSelection()
        advanceUntilIdle()

        assertEquals(reason, viewModel.state.value.status)
        assertEquals(1, viewModel.selection()?.count)
    }

    @Test
    fun addingContentDropsTheSelection() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.marqueeContent(0, wholePage)
        advanceUntilIdle()
        viewModel.armTextInsert()

        assertNull(viewModel.selection())
        assertTrue(viewModel.state.value.contentEdit?.adding != null)
    }

    @Test
    fun noSelectionStartsWhileAnInsertIsArmed() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.armTextInsert()
        viewModel.marqueeContent(0, wholePage)
        advanceUntilIdle()
        viewModel.pressContent(0, onImage, reach = 0.0)
        advanceUntilIdle()

        assertNull(viewModel.selection())
    }

    @Test
    fun noSelectionStartsWhileADialogOrAMoveIsOpen() = runTest {
        val viewModel = opened(RetypableDocument())
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.marqueeContent(0, wholePage)
        advanceUntilIdle()
        assertNull("the retype dialog is open", viewModel.selection())

        viewModel.armTextMove(viewModel.state.value.documentId)
        viewModel.marqueeContent(0, wholePage)
        advanceUntilIdle()
        assertNull("a move is armed", viewModel.selection())
    }

    private suspend fun TestScope.opened(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        viewModel.openContentEdit()
        advanceUntilIdle()
        return viewModel
    }
}
