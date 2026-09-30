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
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Retyping the text a page itself paints, each retype one undoable edit of the shared log. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelContentEditTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    private val onHello = AnnotationPoint(20.0, 155.0)
    private val onWorld = AnnotationPoint(20.0, 125.0)
    private val onNothing = AnnotationPoint(80.0, 40.0)

    @Test
    fun aShownPageHasItsRunsReadForTheOutlines() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.openContentEdit()
        viewModel.contentPageShown(0)
        advanceUntilIdle()

        assertEquals(listOf("Hello", "World"), viewModel.state.value.contentEdit?.runs?.get(0)?.map { it.text })
    }

    @Test
    fun aPageShownTwiceIsReadOnce() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.contentPageShown(0)
        viewModel.contentPageShown(0)
        advanceUntilIdle()
        viewModel.contentPageShown(0)
        advanceUntilIdle()

        assertEquals(listOf(0), document.reads)
    }

    @Test
    fun aTapOnARunOpensItsEditor() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.openContentEdit()
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()

        assertEquals(10L, viewModel.state.value.contentEdit?.editor?.run?.id)
    }

    @Test
    fun aTapOnNoTextSaysSo() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.openContentEdit()
        viewModel.tapContent(0, onNothing, reach = 0.0)
        advanceUntilIdle()

        assertNull(viewModel.state.value.contentEdit?.editor)
        assertEquals(CONTENT_EDIT_MISSED, viewModel.state.value.status)
    }

    @Test
    fun aDocumentThatForbidsContentChangesStaysOutOfTheMode() = runTest {
        val viewModel = openedWith(RetypableDocument(editingAllowed = false))
        viewModel.openContentEdit()
        advanceUntilIdle()

        assertNull(viewModel.state.value.contentEdit)
        assertEquals(CONTENT_EDIT_FORBIDDEN, viewModel.state.value.status)
    }

    @Test
    fun aRetypeReachesTheCoreAndMakesTheDocumentDirty() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextRun(viewModel.state.value.documentId, "Howdy")
        advanceUntilIdle()

        assertEquals(listOf(10L to "Howdy"), document.retypes.map { (run, text) -> run.id to text })
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(TEXT_UPDATED, state.status)
        assertNull(state.contentEdit?.editor)
    }

    @Test
    fun aRetypedRunIsDrawnAndRereadWithItsNewText() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        advanceUntilIdle()
        viewModel.openContentEdit()
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextRun(viewModel.state.value.documentId, "Howdy")
        advanceUntilIdle()

        assertEquals("rendering reads the preview, so it must be rebuilt", listOf("Howdy", "World"), document.drawn.last { it.first == 0 }.second)
        assertEquals("the outline must describe the text now shown", listOf("Howdy", "World"), viewModel.state.value.contentEdit?.runs?.get(0)?.map { it.text })
    }

    @Test
    fun aSecondRetypeSendsTheRunAsItNowReads() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextRun(viewModel.state.value.documentId, "Howdy")
        advanceUntilIdle()
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextRun(viewModel.state.value.documentId, "Hi")
        advanceUntilIdle()

        // The core amends the queued command when the run carries its id and the text it now shows.
        assertEquals(listOf(10L to "Hello", 10L to "Howdy"), document.retypes.map { (run, _) -> run.id to run.text })
    }

    @Test
    fun aRetypeKeepsTheSearchHits() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.search("x")
        advanceUntilIdle()
        viewModel.openContentEdit()
        viewModel.tapContent(0, onWorld, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextRun(viewModel.state.value.documentId, "Earth")
        advanceUntilIdle()

        assertEquals("a retype moves no page", 1, viewModel.state.value.searchHits.size)
    }

    @Test
    fun anUnchangedRetypeQueuesNothing() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextRun(viewModel.state.value.documentId, "Hello")
        advanceUntilIdle()

        assertTrue(document.retypes.isEmpty())
        assertFalse(viewModel.state.value.isDirty)
        assertNull(viewModel.state.value.contentEdit?.editor)
    }

    @Test
    fun aRefusedRetypeKeepsTheEditorOpenWithTheReason() = runTest {
        val refusal = "This text's font cannot show \"é\". Try different characters."
        val viewModel = openedWith(RetypableDocument(refusal = PdfCoreError.Failed(refusal)))
        viewModel.openContentEdit()
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextRun(viewModel.state.value.documentId, "Olé")
        advanceUntilIdle()

        val editor = viewModel.state.value.contentEdit?.editor
        assertNotNull("the reader fixes it by typing something else, not by finding the run again", editor)
        assertEquals(refusal, editor!!.error)
        assertEquals("Olé", editor.text)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun undoingARetypeRedrawsAndRereadsTheRuns() = runTest {
        val document = RetypableDocument()
        val viewModel = openedWith(document)
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        viewModel.openContentEdit()
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextRun(viewModel.state.value.documentId, "Howdy")
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(listOf("Hello", "World"), viewModel.state.value.contentEdit?.runs?.get(0)?.map { it.text })
        assertEquals(listOf("Hello", "World"), document.drawn.last { it.first == 0 }.second)
    }

    @Test
    fun aCommitForAReplacedDocumentIsDropped() = runTest {
        val first = RetypableDocument()
        val second = RetypableDocument()
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(first, second))
        viewModel.open("a.pdf", byteArrayOf(1))
        advanceUntilIdle()
        val staleId = viewModel.state.value.documentId
        viewModel.open("b.pdf", byteArrayOf(2))
        advanceUntilIdle()
        viewModel.openContentEdit()
        viewModel.tapContent(0, onHello, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextRun(staleId, "Howdy")
        advanceUntilIdle()

        assertTrue(first.retypes.isEmpty())
        assertTrue(second.retypes.isEmpty())
    }

    @Test
    fun replacingTheDocumentLeavesTheMode() = runTest {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(RetypableDocument(), RetypableDocument()))
        viewModel.open("a.pdf", byteArrayOf(1))
        advanceUntilIdle()
        viewModel.openContentEdit()
        viewModel.open("b.pdf", byteArrayOf(2))
        advanceUntilIdle()

        assertNull(viewModel.state.value.contentEdit)
    }

    @Test
    fun organizingLeavesTheModeAndTheModeWaitsForTheReader() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.openContentEdit()
        viewModel.openOrganize()

        assertNull(viewModel.state.value.contentEdit)

        viewModel.openContentEdit()
        advanceUntilIdle()

        assertNull("the grid hides the pages a retype would redraw", viewModel.state.value.contentEdit)
    }

    @Test
    fun choosingAnAnnotationToolLeavesTheMode() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.openContentEdit()
        viewModel.setAnnotationTool(AnnotationTool.Highlight)

        assertNull(viewModel.state.value.contentEdit)
        assertEquals(AnnotationTool.Highlight, viewModel.state.value.activeAnnotationTool)
    }

    @Test
    fun enteringTheModeDropsAnArmedTool() = runTest {
        val viewModel = openedWith(RetypableDocument())
        viewModel.setAnnotationTool(AnnotationTool.Ink)
        viewModel.openContentEdit()

        assertEquals("one mode claims a page tap at a time", AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
    }

    private suspend fun TestScope.openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
