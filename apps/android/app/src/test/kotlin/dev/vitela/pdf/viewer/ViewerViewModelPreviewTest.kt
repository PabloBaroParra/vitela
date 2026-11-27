package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.FormFieldValue
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * The reader's low-resolution previews: rendered one at a time, only once no
 * sharp render is running, and redrawn whenever an edit changes the pages.
 * The fakes' renders never convert to a bitmap on the JVM, so these follow
 * the renders the core is asked for.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelPreviewTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun previewsFollowTheSharpWindowAtTheirOwnDensity_nearestFirst() = runTest {
        val document = OrganizableDocument(pageCount = 5)
        val viewModel = openedWith(document)
        viewModel.onReaderPositionChanged(ReaderPosition(2, 2, 2, 1000, 1.0))
        advanceUntilIdle()

        val sizes = viewModel.state.value.pageSizes
        val sharp = renderOrder(2, 2, 5).map { it to renderDpi(sizes[it], 1000) }
        val previews = listOf(2, 3, 1, 4, 0).map { it to previewDpi(sizes[it]) }
        assertEquals(sharp + previews, document.rendered)
    }

    @Test
    fun aPreviewThatCannotBeShownIsNotRetriedForTheSamePages() = runTest {
        val document = OrganizableDocument(pageCount = 3)
        val viewModel = openedWith(document)
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        advanceUntilIdle()
        val sizes = viewModel.state.value.pageSizes
        fun previews() = document.rendered.count { (page, dpi) -> dpi == previewDpi(sizes[page]) }
        val attempted = previews()

        viewModel.onReaderPositionChanged(ReaderPosition(0, 1, 1, 1000, 1.0))
        advanceUntilIdle()

        assertEquals(3, attempted)
        assertEquals("every page already had its one preview attempt", attempted, previews())
    }

    @Test
    fun noPreviewIsRenderedBeforeTheReaderIsMeasured() = runTest {
        val document = OrganizableDocument(pageCount = 3)
        openedWith(document)

        assertEquals(emptyList<Pair<Int, Int>>(), document.rendered)
    }

    @Test
    fun anEditThatRedrawsThePagesRendersTheirPreviewsAgainFromTheRebuiltPreview() = runTest {
        val document = FillableDocument()
        val viewModel = openedWith(document)
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        viewModel.openFormFields()
        advanceUntilIdle()
        val before = document.drawn.size

        viewModel.fillFormField(viewModel.state.value.documentId, 1, FormFieldValue.Text("Grace"))
        advanceUntilIdle()

        val after = document.drawn.drop(before)
        // Pages 0 and 1 sharp, then their previews: none of them may keep the old value.
        assertEquals(listOf(0, 1, 0, 1), after.map { it.first })
        assertTrue(after.all { it.second[1L] == FormFieldValue.Text("Grace") })
    }

    @Test
    fun previewsStopWithTheDocument() = runTest {
        val document = OrganizableDocument(pageCount = 3)
        val viewModel = openedWith(document)
        val sizes = viewModel.state.value.pageSizes
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        viewModel.closeDocument()
        advanceUntilIdle()

        assertTrue(document.rendered.isNotEmpty())
        assertTrue(document.rendered.none { (page, dpi) -> dpi == previewDpi(sizes[page]) })
    }

    private suspend fun TestScope.openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
