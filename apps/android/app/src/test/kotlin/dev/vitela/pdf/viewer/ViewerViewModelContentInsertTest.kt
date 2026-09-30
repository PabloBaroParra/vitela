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
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Adding new text and images to a page, from Edit content: Add text or Add
 * image arms a tap, the tap names the top-left corner, and each insert is one
 * undoable edit.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelContentInsertTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // RetypableDocument paints nothing at (50, 180) on either page, and "Hello" at (10, 150)–(50, 162) on page 0.
    private val clear = AnnotationPoint(50.0, 180.0)
    private val onHello = AnnotationPoint(20.0, 155.0)
    private val png = byteArrayOf(1, 2, 3)

    @Test
    fun addTextArmsATapAndClosesAnyDialog() = runTest {
        val viewModel = editing(RetypableDocument())
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.armTextInsert()

        val mode = viewModel.state.value.contentEdit
        assertEquals(ContentAddition.Text, mode?.adding)
        assertNull(mode?.editor)
        assertEquals(TEXT_INSERT_PROMPT, viewModel.state.value.status)
    }

    @Test
    fun theArmedTapOpensTheTextDialogThereAndInsertsNothingYet() = runTest {
        val document = RetypableDocument()
        val viewModel = editing(document)
        viewModel.armTextInsert()
        // Over existing text too: an armed insert claims the tap, it does not open the run.
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()

        val mode = viewModel.state.value.contentEdit
        assertNull("one tap, one placement", mode?.adding)
        assertNull(mode?.editor)
        assertEquals(TextInserter(0, onHello), mode?.inserter)
        assertTrue(document.inserts.isEmpty())
    }

    @Test
    fun insertingTextPutsItsTopLeftCornerOnTheTap() = runTest {
        val document = RetypableDocument()
        val viewModel = placedText(document)
        viewModel.insertText(viewModel.state.value.documentId, "New line", "20")
        advanceUntilIdle()

        assertEquals(listOf(Triple(0, "New line", AnnotationRect(50.0, 160.0, 20.0, 20.0))), document.inserts)
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(TEXT_INSERTED, state.status)
        assertNull(state.contentEdit?.inserter)
        assertTrue("the new run is outlined as the core reports it", state.contentEdit?.runs?.get(0).orEmpty().any { it.text == "New line" })
    }

    @Test
    fun insertedTextRedrawsThePage() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        advanceUntilIdle()
        val renders = document.drawn.size
        viewModel.openContentEdit()
        advanceUntilIdle()
        viewModel.armTextInsert()
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()
        viewModel.insertText(viewModel.state.value.documentId, "New line", "14")
        advanceUntilIdle()

        assertTrue("only the renderer can paint the new text", document.drawn.drop(renders).any { (page, texts) -> page == 0 && "New line" in texts })
    }

    @Test
    fun blankOrOversizedTextKeepsTheDialogWithWhatWasTyped() = runTest {
        val document = RetypableDocument()
        val viewModel = placedText(document)
        val documentId = viewModel.state.value.documentId
        viewModel.insertText(documentId, "   ", "14")
        advanceUntilIdle()
        assertEquals(TextInserter(0, clear, "   ", "14", TEXT_INSERT_EMPTY), viewModel.state.value.contentEdit?.inserter)

        viewModel.insertText(documentId, "Big", "73")
        advanceUntilIdle()
        assertEquals(TextInserter(0, clear, "Big", "73", TEXT_SIZE_INVALID), viewModel.state.value.contentEdit?.inserter)
        assertTrue(document.inserts.isEmpty())
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun aRefusedInsertKeepsTheDialogWithTheReason() = runTest {
        val refusal = "Helvetica cannot show \"語\". Try different characters."
        val viewModel = placedText(RetypableDocument(refusal = PdfCoreError.Failed(refusal)))
        viewModel.insertText(viewModel.state.value.documentId, "語", "14")
        advanceUntilIdle()

        assertEquals(TextInserter(0, clear, "語", "14", refusal), viewModel.state.value.contentEdit?.inserter)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun aDoubleConfirmInsertsOnce() = runTest {
        val document = RetypableDocument()
        val viewModel = placedText(document)
        val documentId = viewModel.state.value.documentId
        viewModel.insertText(documentId, "Once", "14")
        viewModel.insertText(documentId, "Once", "14")
        advanceUntilIdle()

        assertEquals(1, document.inserts.size)
    }

    @Test
    fun aTextDialogBuiltForAReplacedDocumentInsertsNothing() = runTest {
        val document = RetypableDocument()
        val viewModel = placedText(document)
        viewModel.insertText(viewModel.state.value.documentId + 1, "Stale", "14")
        advanceUntilIdle()

        assertTrue(document.inserts.isEmpty())
    }

    @Test
    fun undoingAnInsertTakesTheTextBackOff() = runTest {
        val viewModel = placedText(RetypableDocument())
        viewModel.insertText(viewModel.state.value.documentId, "New line", "14")
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertFalse(viewModel.state.value.contentEdit?.runs?.get(0).orEmpty().any { it.text == "New line" })
    }

    @Test
    fun theArmedTapPlacesAnImageAtTheCoresSize() = runTest {
        val document = RetypableDocument()
        val viewModel = editing(document)
        viewModel.armImageInsert(png)
        assertEquals(IMAGE_INSERT_PROMPT, viewModel.state.value.status)
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()

        val (page, bytes, bounds) = document.imageInserts.single()
        assertEquals(1, page)
        assertSame("the bytes chosen are the bytes sent", png, bytes)
        assertEquals("the core sizes it; the tap is its top-left corner", AnnotationRect(50.0, 160.0, 40.0, 20.0), bounds)
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertEquals(IMAGE_INSERTED, state.status)
        assertNull("one tap, one image", state.contentEdit?.adding)
        assertEquals(bounds, state.contentEdit?.images?.get(1)?.single()?.bounds)
    }

    @Test
    fun aDoubleTapInsertsOneImage() = runTest {
        val document = RetypableDocument()
        val viewModel = editing(document)
        viewModel.armImageInsert(png)
        viewModel.tapContent(0, clear, reach = 0.0)
        viewModel.tapContent(0, AnnotationPoint(60.0, 190.0), reach = 0.0)
        advanceUntilIdle()

        assertEquals(1, document.imageInserts.size)
    }

    @Test
    fun anImageTheCoreCannotReadIsReportedAndDisarmed() = runTest {
        val refusal = "This image cannot be inserted."
        val viewModel = editing(RetypableDocument(refusal = PdfCoreError.Failed(refusal)))
        viewModel.armImageInsert(png)
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()

        assertEquals(refusal, viewModel.state.value.status)
        assertNull(viewModel.state.value.contentEdit?.adding)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun anImageChosenAfterTheModeClosedArmsNothing() = runTest {
        val viewModel = editing(RetypableDocument())
        viewModel.closeContentEdit()
        viewModel.armImageInsert(png)

        assertNull(viewModel.state.value.contentEdit)
    }

    @Test
    fun cancellingAnInsertLeavesTheNextTapToOpenWhatItHits() = runTest {
        val document = RetypableDocument()
        val viewModel = editing(document)
        viewModel.armImageInsert(png)
        viewModel.cancelInsert()
        assertEquals(INSERT_CANCELLED, viewModel.state.value.status)
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()

        assertTrue(document.imageInserts.isEmpty())
        assertEquals("Hello", viewModel.state.value.contentEdit?.editor?.run?.text)
    }

    @Test
    fun armingAnInsertDisarmsAnImageMove() = runTest {
        val viewModel = editing(RetypableDocument())
        viewModel.tapContent(0, AnnotationPoint(20.0, 30.0), reach = 0.0)
        advanceUntilIdle()
        viewModel.armImageMove(viewModel.state.value.documentId)
        viewModel.armTextInsert()

        val mode = viewModel.state.value.contentEdit
        assertNull(mode?.movingImage)
        assertEquals(ContentAddition.Text, mode?.adding)
    }

    private suspend fun TestScope.placedText(document: PdfDocument): ViewerViewModel = editing(document).also { viewModel ->
        viewModel.armTextInsert()
        viewModel.tapContent(0, clear, reach = 0.0)
        advanceUntilIdle()
    }

    private suspend fun TestScope.editing(document: PdfDocument): ViewerViewModel = openedWith(document).also { viewModel ->
        viewModel.openContentEdit()
        advanceUntilIdle()
    }

    private suspend fun TestScope.openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
