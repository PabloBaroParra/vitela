package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
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

/**
 * The core's content-edit checkpoint (retyping moved text, moving retyped text,
 * undoing across either): it renumbers every item on every page, so the shell
 * has to read the pages it holds again and let go of what it held.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelContentCheckpointTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // RetypableDocument: page 0 "Hello" (10, 150)–(50, 162), "World", image 30; page 1 "Page two" (10, 150)–(70, 162).
    private val onHello = AnnotationPoint(20.0, 156.0)
    private val onPageTwo = AnnotationPoint(20.0, 156.0)
    private val clear = AnnotationPoint(50.0, 180.0)

    @Test
    fun aCheckpointRereadsEveryPageAlreadyReadNotOnlyTheEditedOne() = runTest {
        val document = CheckpointingDocument()
        val viewModel = editing(document)
        readPageOne(viewModel)
        document.checkpointOnNextEdit = true
        retypeHello(viewModel, "Howdy")

        val blocks = viewModel.state.value.contentEdit?.blocks
        assertEquals("the other page was read at the new generation", 1000L + 20, blocks?.get(1)?.first()?.id)
        assertEquals(1000L + 10, blocks?.get(0)?.first()?.id)
        assertEquals(TEXT_UPDATED, viewModel.state.value.status)
    }

    @Test
    fun anEditBuiltFromThePageReadAfterTheCheckpointIsAccepted() = runTest {
        val document = CheckpointingDocument()
        val viewModel = editing(document)
        readPageOne(viewModel)
        document.checkpointOnNextEdit = true
        retypeHello(viewModel, "Howdy")

        viewModel.tapContent(1, onPageTwo, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "Page deux")
        advanceUntilIdle()

        assertTrue("nothing was sent with an id the new base does not hold: ${document.staleRefusals}", document.staleRefusals.isEmpty())
        assertEquals(TEXT_UPDATED, viewModel.state.value.status)
    }

    @Test
    fun withoutACheckpointOnlyTheEditedPageIsReadAgain() = runTest {
        val document = RetypableDocument()
        val viewModel = editing(CheckpointingDocument(document))
        readPageOne(viewModel)
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        val before = document.reads.size
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "Howdy")
        advanceUntilIdle()

        assertEquals(listOf(0), document.reads.drop(before))
    }

    @Test
    fun aCheckpointDropsWhatTheModeHeldAsItWasRead() = runTest {
        val document = CheckpointingDocument()
        val viewModel = editing(document)
        viewModel.pressContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        assertNotNull(viewModel.state.value.contentEdit?.selection)
        document.checkpointOnNextEdit = true
        viewModel.moveContentSelection(0, 5.0, -5.0)
        advanceUntilIdle()

        val mode = viewModel.state.value.contentEdit
        assertNull("its items belong to a base that is gone", mode?.selection)
        assertEquals(SELECTION_MOVED, viewModel.state.value.status)
        assertTrue(document.staleRefusals.isEmpty())
        assertEquals(1010L, mode?.blocks?.get(0)?.first()?.id)
    }

    @Test
    fun withoutACheckpointTheSelectionFollowsAMoveAsBefore() = runTest {
        val document = CheckpointingDocument()
        val viewModel = editing(document)
        viewModel.pressContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.moveContentSelection(0, 5.0, -5.0)
        advanceUntilIdle()

        assertEquals(1, viewModel.state.value.contentEdit?.selection?.count)
    }

    @Test
    fun aPasteThatCheckpointsStandsButSelectsNothing() = runTest {
        val document = CheckpointingDocument()
        val viewModel = editing(document)
        viewModel.pressContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.copyContentSelection()
        advanceUntilIdle()
        viewModel.armContentPaste()
        document.checkpointOnNextEdit = true
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()

        val state = viewModel.state.value
        assertNull("the ids it would have diffed against describe a parse that is gone", state.contentEdit?.selection)
        assertEquals(CONTENT_PASTED, state.status)
        assertTrue(state.isDirty)
        assertTrue("the pasted line is outlined as the new base reports it", state.contentEdit?.blocks?.get(1).orEmpty().any { it.text == "Hello" })
    }

    @Test
    fun aPasteWithoutACheckpointStillSelectsWhatItAdded() = runTest {
        val viewModel = editing(CheckpointingDocument())
        viewModel.pressContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.copyContentSelection()
        advanceUntilIdle()
        viewModel.armContentPaste()
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()

        assertEquals(listOf("Hello"), viewModel.state.value.contentEdit?.selection?.blocks?.map { it.text })
    }

    @Test
    fun undoAcrossACheckpointReadsEveryPageAgain() = runTest {
        val document = CheckpointingDocument()
        val viewModel = editing(document)
        readPageOne(viewModel)
        retypeHello(viewModel, "Howdy")
        document.crossOnNextHistory = true
        viewModel.undoAnnotations()
        advanceUntilIdle()

        val blocks = viewModel.state.value.contentEdit?.blocks
        assertEquals(1000L + 20, blocks?.get(1)?.first()?.id)
        assertEquals(listOf("Hello", "World"), blocks?.get(0)?.map { it.text })
        assertFalse(viewModel.state.value.canUndoAnnotations)

        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "Again")
        advanceUntilIdle()
        assertTrue(document.staleRefusals.isEmpty())
    }

    @Test
    fun theTextDialogWarnsOfASaveOnlyWhereTheCoreCannotCheckpoint() = runTest {
        for (canCheckpoint in listOf(true, false)) {
            val viewModel = editing(CheckpointingDocument(canCheckpoint = canCheckpoint))
            viewModel.tapContent(0, onHello, reach = 0.0)
            advanceUntilIdle()

            val editor = viewModel.state.value.contentEdit?.editor
            assertEquals(canCheckpoint, editor?.canCheckpoint)
            assertEquals(!canCheckpoint, "needs a save first" in textDialogHelp(editor?.canCheckpoint == true))
        }
    }

    private suspend fun TestScope.readPageOne(viewModel: ViewerViewModel) {
        viewModel.tapContent(1, clear, reach = 0.0)
        advanceUntilIdle()
        assertNotNull(viewModel.state.value.contentEdit?.blocks?.get(1))
    }

    private suspend fun TestScope.retypeHello(viewModel: ViewerViewModel, text: String) {
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextBlock(viewModel.state.value.documentId, text)
        advanceUntilIdle()
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
