package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Direct page navigation: picking a numbered page scrolls the reader there, and is never a document edit. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelPageNavigationTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun pickingAPageScrollsTheReaderToIt() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 5))
        viewModel.consumeScrollTarget()

        viewModel.goToPage(3)

        assertEquals(3, viewModel.state.value.scrollTarget)
    }

    @Test
    fun aPageOutsideTheDocumentIsBoundedToIt() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 5))

        viewModel.goToPage(9)
        assertEquals(4, viewModel.state.value.scrollTarget)
        viewModel.goToPage(-2)
        assertEquals(0, viewModel.state.value.scrollTarget)
    }

    @Test
    fun navigationIsNotAnEditAndLeavesHistoryAlone() = runTest {
        val document = OrganizableDocument(pageCount = 5)
        val viewModel = openedWith(document)

        viewModel.goToPage(2)
        advanceUntilIdle()

        assertTrue(document.edits.isEmpty())
        assertFalse(viewModel.state.value.canUndoAnnotations)
    }

    @Test
    fun theGridIgnoresNavigationSoNoTargetIsLeftForTheReader() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 5))
        viewModel.consumeScrollTarget()
        viewModel.openOrganize()

        viewModel.goToPage(3)

        assertNull(viewModel.state.value.scrollTarget)
        assertFalse(pageNavigationEnabled(viewModel.state.value))
    }

    @Test
    fun previousAndNextStepFromTheCurrentPage() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 5))
        viewModel.consumeScrollTarget()
        viewModel.onReaderPositionChanged(ReaderPosition(first = 2, last = 2, current = 2, viewportWidthPx = 1000, zoomFactor = 1.0))

        viewModel.navigate(1)
        assertEquals(3, viewModel.state.value.scrollTarget)
        viewModel.navigate(-1)
        assertEquals(1, viewModel.state.value.scrollTarget)
    }

    @Test
    fun theGridIgnoresPreviousAndNextSoNoTargetIsLeftForTheReader() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 5))
        viewModel.consumeScrollTarget()
        viewModel.openOrganize()

        viewModel.navigate(1)

        assertNull(viewModel.state.value.scrollTarget)
    }

    @Test
    fun navigationNeedsPagesAndAnIdleReader() {
        assertFalse(pageNavigationEnabled(ViewerState(pageCount = 0)))
        assertFalse(pageNavigationEnabled(ViewerState(pageCount = 3, isLoading = true)))
        assertTrue(pageNavigationEnabled(ViewerState(pageCount = 3)))
    }

    private suspend fun TestScope.openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
