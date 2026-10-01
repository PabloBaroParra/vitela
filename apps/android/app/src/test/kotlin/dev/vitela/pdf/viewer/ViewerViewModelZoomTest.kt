package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test

/** Continuous zoom (pinch) and the two fits: a zoom is a view change, never an edit. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelZoomTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun fitPageShrinksTheCurrentPageToTheViewportHeightAndScrollsToIt() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 3))
        viewModel.consumeScrollTarget()
        // Page 1 is 101 x 200 pt: at fit-to-width on a 1000 px column it is
        // 1980 px tall, so a 1200 px viewport needs 1200 / 1980 of that.
        viewModel.onReaderPositionChanged(ReaderPosition(0, 1, 1, 1000, 1.0, viewportHeightPx = 1200))

        viewModel.fitPage()

        assertEquals(1200.0 * 101.0 / (1000.0 * 200.0), viewModel.state.value.zoomFactor, 1e-9)
        assertEquals(1, viewModel.state.value.scrollTarget)
    }

    @Test
    fun fitPageNeverZoomsPastFitToWidth() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 3))
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0, viewportHeightPx = 5000))

        viewModel.fitPage()

        assertEquals(DEFAULT_ZOOM_FACTOR, viewModel.state.value.zoomFactor, 0.0)
    }

    @Test
    fun fitPageWaitsForAMeasuredViewport() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 3))
        viewModel.consumeScrollTarget()
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))

        viewModel.fitPage()

        assertEquals(DEFAULT_ZOOM_FACTOR, viewModel.state.value.zoomFactor, 0.0)
        assertNull(viewModel.state.value.scrollTarget)
    }

    @Test
    fun fitWidthReturnsToTheDefaultLayout() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 3))
        viewModel.zoomIn()
        viewModel.zoomIn()

        viewModel.fitWidth()

        assertEquals(DEFAULT_ZOOM_FACTOR, viewModel.state.value.zoomFactor, 0.0)
    }

    @Test
    fun aPinchLandsOnAnyFactorBetweenTheRungsButStaysClamped() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 3))

        viewModel.setZoom(1.37)
        assertEquals(1.37, viewModel.state.value.zoomFactor, 0.0)
        viewModel.setZoom(40.0)
        assertEquals(MAX_ZOOM_FACTOR, viewModel.state.value.zoomFactor, 0.0)
    }

    @Test
    fun zoomingIsNotAnEdit() = runTest {
        val document = OrganizableDocument(pageCount = 3)
        val viewModel = openedWith(document)
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0, viewportHeightPx = 1200))

        viewModel.setZoom(2.5)
        viewModel.fitPage()
        viewModel.fitWidth()
        advanceUntilIdle()

        assertEquals(emptyList<Any>(), document.edits)
    }

    @Test
    fun withoutADocumentThereIsNothingToZoom() {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore())

        viewModel.setZoom(2.0)
        viewModel.fitPage()

        assertEquals(DEFAULT_ZOOM_FACTOR, viewModel.state.value.zoomFactor, 0.0)
    }

    private suspend fun TestScope.openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
