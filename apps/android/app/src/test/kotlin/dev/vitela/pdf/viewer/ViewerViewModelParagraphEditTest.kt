package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
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
 * A paragraph — lines stacked a steady leading apart — read, retyped, moved,
 * deleted and selected as one text block (PR #568's core paragraphs, on Android).
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelParagraphEditTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // RetypableDocument(paragraphs = true), page 1: "First line" (10, 60) over "second line" (10, 46).
    private val onSecondLine = AnnotationPoint(20.0, 50.0)
    private val betweenTheLines = AnnotationPoint(20.0, 59.0)
    private val elsewhereOnPageOne = AnnotationPoint(30.0, 20.0)

    private fun ViewerViewModel.paragraphOnPageOne() =
        state.value.contentEdit?.blocks?.get(1)?.single { it.runs.first().id == 23L }

    @Test
    fun aTapOnAParagraphsLineOpensTheWholeParagraph() = runTest {
        val viewModel = openedWith(RetypableDocument(paragraphs = true))
        viewModel.openContentEdit()
        viewModel.tapContent(1, onSecondLine, reach = 0.0)
        advanceUntilIdle()

        val block = viewModel.state.value.contentEdit?.editor?.block
        assertNotNull(block)
        assertTrue(block!!.isParagraph)
        assertEquals(2, block.lineCount)
        assertEquals(listOf(23L, 24L), block.runs.map { it.id })
        assertEquals("First line\nsecond line", viewModel.state.value.contentEdit?.editor?.text)
    }

    @Test
    fun theGapBetweenAParagraphsLinesBelongsToIt() = runTest {
        val viewModel = openedWith(RetypableDocument(paragraphs = true))
        viewModel.openContentEdit()
        viewModel.tapContent(1, betweenTheLines, reach = 0.0)
        advanceUntilIdle()

        assertEquals(23L, viewModel.state.value.contentEdit?.editor?.block?.id)
    }

    @Test
    fun aParagraphRetypeSendsEveryLinesRunsAndTheTypedLineBreaks() = runTest {
        val document = RetypableDocument(paragraphs = true)
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.tapContent(1, onSecondLine, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "One\nTwo\nThree")
        advanceUntilIdle()

        val (sent, text) = document.retypes.single()
        assertEquals(listOf(23L, 24L), sent.runs.map { it.id })
        assertEquals("One\nTwo\nThree", text)
        assertTrue(viewModel.state.value.isDirty)
        assertTrue(viewModel.state.value.canUndoAnnotations)
        assertEquals(TEXT_UPDATED, viewModel.state.value.status)
    }

    @Test
    fun aRetypedParagraphIsRereadWithTheLinesItNowHas() = runTest {
        val viewModel = openedWith(RetypableDocument(paragraphs = true))
        viewModel.openContentEdit()
        viewModel.tapContent(1, onSecondLine, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "One\nTwo\nThree")
        advanceUntilIdle()

        val paragraph = viewModel.paragraphOnPageOne()
        assertEquals(3, paragraph?.lineCount)
        assertEquals("One\nTwo\nThree", paragraph?.text)
        assertFalse("a commit closes the dialog", viewModel.state.value.contentEdit?.editor != null)
    }

    @Test
    fun aSecondRetypeSendsTheParagraphAsItNowReadsGrownLineIncluded() = runTest {
        val document = RetypableDocument(paragraphs = true)
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.tapContent(1, onSecondLine, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "One\nTwo\nThree")
        advanceUntilIdle()
        viewModel.tapContent(1, onSecondLine, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "One\nTwo")
        advanceUntilIdle()

        val (sent, text) = document.retypes.last()
        assertEquals("the runs the core reports after the first retype", 3, sent.runs.size)
        assertEquals(listOf(23L, 24L), sent.runs.take(2).map { it.id })
        assertEquals("One\nTwo", text)
    }

    @Test
    fun movingARegrownParagraphSendsEveryRunAsItNowReads() = runTest {
        val document = RetypableDocument(paragraphs = true)
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.tapContent(1, onSecondLine, reach = 0.0)
        advanceUntilIdle()
        viewModel.retypeTextBlock(viewModel.state.value.documentId, "One\nTwo\nThree")
        advanceUntilIdle()
        val grown = viewModel.paragraphOnPageOne()!!
        viewModel.tapContent(1, onSecondLine, reach = 0.0)
        advanceUntilIdle()
        viewModel.armTextMove(viewModel.state.value.documentId)
        viewModel.tapContent(1, elsewhereOnPageOne, reach = 0.0)
        advanceUntilIdle()

        val (sent, _, _) = document.textMoves.single()
        assertEquals(grown.runs.map { it.id }, sent.runs.map { it.id })
        assertEquals(3, sent.runs.size)
    }

    @Test
    fun deletingAParagraphRemovesEveryLine() = runTest {
        val document = RetypableDocument(paragraphs = true)
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.tapContent(1, onSecondLine, reach = 0.0)
        advanceUntilIdle()
        viewModel.deleteTextBlock(viewModel.state.value.documentId)
        advanceUntilIdle()

        assertEquals(listOf(23L, 24L), document.textDeletes.single().runs.map { it.id })
        assertNull(viewModel.state.value.contentEdit?.blocks?.get(1)?.find { it.id == 23L })
    }

    @Test
    fun aMarqueeOverOneLineSelectsTheParagraphWithEveryRun() = runTest {
        val document = RetypableDocument(paragraphs = true)
        val viewModel = openedWith(document)
        viewModel.openContentEdit()
        viewModel.marqueeContent(1, AnnotationRect(0.0, 44.0, 100.0, 6.0))
        advanceUntilIdle()
        viewModel.deleteContentSelection()
        advanceUntilIdle()

        val sent = document.selectionDeletes.single()
        assertEquals(listOf(23L), sent.blocks.map { it.id })
        assertEquals(listOf(23L, 24L), sent.blocks.single().runs.map { it.id })
    }

    private suspend fun TestScope.openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        return viewModel
    }
}
