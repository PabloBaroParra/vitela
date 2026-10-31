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
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Copying, cutting and pasting page content (B25 T-209): the selection's Copy
 * and Cut chips fill a clip the shell holds, and Paste content arms a tap that
 * names the clip's top-left corner.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelContentClipboardTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // RetypableDocument, page 0: blocks "Hello" (10, 150)–(50, 162) and "World" (10, 120)–(50, 132), image 30 at
    // (10, 20)–(40, 50), rule 40 and box 41. Nothing is painted at (50, 180) on either page.
    private val onHello = AnnotationPoint(20.0, 156.0)
    private val onImage = AnnotationPoint(20.0, 30.0)
    private val clear = AnnotationPoint(50.0, 180.0)

    @Test
    fun copyingHoldsAClipAndChangesNothingInTheDocument() = runTest {
        val document = RetypableDocument()
        val viewModel = selecting(document, onHello, onImage)
        viewModel.copyContentSelection()
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(1, document.copies.size)
        assertEquals(copiedMessage("Copied", 2) + CONTENT_COPIED_HINT, state.status)
        assertTrue(state.contentEdit?.hasClip == true)
        assertEquals("the selection stays, so it can be cut or deleted next", 2, state.contentEdit?.selection?.count)
        assertFalse("a copy is not an edit", state.isDirty)
        assertFalse(state.canUndoAnnotations)
    }

    @Test
    fun copyingOneItemSaysItem() {
        assertEquals("Copied 1 item.", copiedMessage("Copied", 1))
        assertEquals("Cut 3 items.", copiedMessage("Cut", 3))
    }

    @Test
    fun aRefusedCopyHoldsNoClipAndShowsTheReason() = runTest {
        val viewModel = selecting(RetypableDocument(refusal = PdfCoreError.Failed("Save the document before copying content you have edited.")), onHello)
        viewModel.copyContentSelection()
        advanceUntilIdle()

        assertEquals("Save the document before copying content you have edited.", viewModel.state.value.status)
        assertFalse(viewModel.state.value.contentEdit?.hasClip == true)
        assertEquals(1, viewModel.state.value.contentEdit?.selection?.count)
    }

    @Test
    fun cuttingRemovesTheSelectionAsOneUndoableEditAndHoldsTheClip() = runTest {
        val document = RetypableDocument()
        val viewModel = selecting(document, onHello, onImage)
        viewModel.cutContentSelection()
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(1, document.cuts.size)
        assertEquals(copiedMessage("Cut", 2) + CONTENT_COPIED_HINT, state.status)
        assertTrue(state.contentEdit?.hasClip == true)
        assertNull("what was cut is no longer there to select", state.contentEdit?.selection)
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(listOf("World"), state.contentEdit?.blocks?.get(0).orEmpty().map { it.text })
        assertEquals(emptyList<Long>(), state.contentEdit?.images?.get(0).orEmpty().map { it.id })
    }

    @Test
    fun undoingACutKeepsTheClip() = runTest {
        val viewModel = selecting(RetypableDocument(), onHello)
        viewModel.cutContentSelection()
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertTrue(viewModel.state.value.contentEdit?.hasClip == true)
        assertEquals(listOf("Hello", "World"), viewModel.state.value.contentEdit?.blocks?.get(0).orEmpty().map { it.text })
    }

    @Test
    fun aRefusedCutGivesTheSelectionBackAndHoldsNoClip() = runTest {
        val viewModel = selecting(RetypableDocument(refusal = PdfCoreError.Failed("Undo the paste to take it back, or save the document before deleting it.")), onHello)
        viewModel.cutContentSelection()
        advanceUntilIdle()

        assertEquals("Undo the paste to take it back, or save the document before deleting it.", viewModel.state.value.status)
        assertEquals(1, viewModel.state.value.contentEdit?.selection?.count)
        assertFalse(viewModel.state.value.contentEdit?.hasClip == true)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun pasteContentArmsATapOnlyWhileAClipIsHeld() = runTest {
        val viewModel = editing(RetypableDocument())
        viewModel.armContentPaste()

        assertNull("nothing copied, nothing to paste", viewModel.state.value.contentEdit?.adding)
        assertFalse(viewModel.state.value.contentEdit?.hasClip == true)

        copied(viewModel, onHello)
        viewModel.armContentPaste()

        val mode = viewModel.state.value.contentEdit
        assertEquals(ContentAddition.Paste, mode?.adding)
        assertNull("arming ends the selection", mode?.selection)
        assertEquals(CONTENT_PASTE_PROMPT, viewModel.state.value.status)
    }

    @Test
    fun theArmedTapPastesTheClipWithItsTopLeftCornerOnTheTap() = runTest {
        val document = RetypableDocument()
        val viewModel = selecting(document, onHello)
        viewModel.copyContentSelection()
        advanceUntilIdle()
        viewModel.armContentPaste()
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()

        val (page, _, corner) = document.pastes.single()
        assertEquals(1, page)
        assertEquals(clear.x to clear.y, corner)
        val state = viewModel.state.value
        assertNull("one tap, one placement", state.contentEdit?.adding)
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertTrue("the pasted line is outlined as the core reports it", state.contentEdit?.blocks?.get(1).orEmpty().any { it.text == "Hello" && it.bounds.x == 50.0 })
    }

    @Test
    fun whatWasPastedIsSelectedSoItCanBeDraggedIntoPlace() = runTest {
        val document = RetypableDocument()
        val viewModel = selecting(document, onHello, onImage)
        viewModel.copyContentSelection()
        advanceUntilIdle()
        viewModel.armContentPaste()
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()

        val selection = viewModel.state.value.contentEdit?.selection
        assertEquals(1, selection?.pageIndex)
        assertEquals(listOf("Hello"), selection?.blocks?.map { it.text })
        assertEquals(1, selection?.images?.size)
        assertTrue("only the new items: not the page's own", selection?.blocks.orEmpty().none { it.text == "Page two" })
        assertEquals(pastedMessage(2), viewModel.state.value.status)

        viewModel.moveContentSelection(1, 5.0, -5.0)
        advanceUntilIdle()

        assertEquals(1, document.selectionMoves.size)
        val moved = document.selectionMoves.single().first
        assertEquals(listOf("Hello"), moved.blocks.map { it.text })
        assertTrue("the paste's own items, not the originals", moved.blocks.single().id >= 100 && moved.images.single().id >= 100)
    }

    @Test
    fun aPasteOntoThePageItWasCopiedFromSelectsOnlyTheNewItems() = runTest {
        val document = RetypableDocument()
        val viewModel = selecting(document, onImage)
        viewModel.copyContentSelection()
        advanceUntilIdle()
        viewModel.armContentPaste()
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        val selection = viewModel.state.value.contentEdit?.selection
        assertEquals(1, selection?.count)
        assertTrue("the original image is not in it", selection?.images.orEmpty().none { it.id == 30L })
        assertEquals(2, viewModel.state.value.contentEdit?.images?.get(0)?.size)
    }

    @Test
    fun aPasteIsOneUndoStep() = runTest {
        val viewModel = selecting(RetypableDocument(), onHello)
        viewModel.copyContentSelection()
        advanceUntilIdle()
        viewModel.armContentPaste()
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertFalse(viewModel.state.value.canUndoAnnotations)
        assertEquals(listOf("Page two", "Two runs"), viewModel.state.value.contentEdit?.blocks?.get(1).orEmpty().map { it.text })
        assertTrue("the clip outlives the undo", viewModel.state.value.contentEdit?.hasClip == true)
        assertNull("the pasted items are gone, so is the selection", viewModel.state.value.contentEdit?.selection)
    }

    @Test
    fun aRefusedPasteShowsTheReasonAndSelectsNothing() = runTest {
        val document = RetypableDocument(pasteRefusal = PdfCoreError.Failed("This document does not permit content changes."))
        val viewModel = selecting(document, onHello)
        viewModel.copyContentSelection()
        advanceUntilIdle()
        viewModel.armContentPaste()
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()

        assertEquals("This document does not permit content changes.", viewModel.state.value.status)
        assertNull(viewModel.state.value.contentEdit?.adding)
        assertNull(viewModel.state.value.contentEdit?.selection)
        assertFalse(viewModel.state.value.isDirty)
        assertTrue("a refusal keeps the clip", viewModel.state.value.contentEdit?.hasClip == true)
    }

    @Test
    fun cancelInsertDisarmsAPaste() = runTest {
        val viewModel = selecting(RetypableDocument(), onHello)
        viewModel.copyContentSelection()
        advanceUntilIdle()
        viewModel.armContentPaste()
        viewModel.cancelInsert()

        assertNull(viewModel.state.value.contentEdit?.adding)
        assertEquals(CONTENT_PASTE_CANCELLED, viewModel.state.value.status)
        assertTrue("cancelling keeps the clip", viewModel.state.value.contentEdit?.hasClip == true)
    }

    @Test
    fun theClipOutlivesTheDocumentSoItPastesIntoAnotherOne() = runTest {
        val first = RetypableDocument()
        val second = RetypableDocument()
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(first, second))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        viewModel.openContentEdit()
        advanceUntilIdle()
        viewModel.pressContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.copyContentSelection()
        advanceUntilIdle()
        val firstId = viewModel.state.value.documentId

        viewModel.open("b.pdf", byteArrayOf(2))
        viewModel.state.first { it.documentId != firstId && !it.isLoading }
        advanceUntilIdle()
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        viewModel.openContentEdit()
        advanceUntilIdle()

        assertTrue("the new document's mode knows a clip is held", viewModel.state.value.contentEdit?.hasClip == true)
        viewModel.armContentPaste()
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()

        assertEquals(1, second.pastes.size)
        assertEquals(0, first.pastes.size)
        assertNotNull(viewModel.state.value.contentEdit?.selection)
    }

    @Test
    fun theSameClipPastesAgainAndAgain() = runTest {
        val document = RetypableDocument()
        val viewModel = selecting(document, onHello)
        viewModel.copyContentSelection()
        advanceUntilIdle()
        repeat(2) {
            viewModel.armContentPaste()
            viewModel.tapContent(1, clear, reach = 0.0)
            advanceUntilIdle()
        }

        assertEquals(2, document.pastes.size)
        assertSame(document.pastes[0].second, document.pastes[1].second)
    }

    @Test
    fun copyAndCutNeedASelection() = runTest {
        val document = RetypableDocument()
        val viewModel = editing(document)
        viewModel.copyContentSelection()
        viewModel.cutContentSelection()
        advanceUntilIdle()

        assertTrue(document.copies.isEmpty() && document.cuts.isEmpty())
    }

    private suspend fun TestScope.copied(viewModel: ViewerViewModel, at: AnnotationPoint) {
        viewModel.pressContent(0, at, reach = 0.0)
        advanceUntilIdle()
        viewModel.copyContentSelection()
        advanceUntilIdle()
    }

    private suspend fun TestScope.selecting(document: PdfDocument, vararg at: AnnotationPoint): ViewerViewModel {
        val viewModel = editing(document)
        viewModel.pressContent(0, at.first(), reach = 0.0)
        advanceUntilIdle()
        for (point in at.drop(1)) {
            viewModel.tapContent(0, point, reach = 0.0)
            advanceUntilIdle()
        }
        return viewModel
    }

    private suspend fun TestScope.editing(document: PdfDocument): ViewerViewModel {
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
