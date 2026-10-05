package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationEdit
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelCloseTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun closingACleanDocumentReleasesItAndReturnsHome() = runTest {
        val core = CloseCore()
        val viewer = dispatchers.viewModel(core)
        open(viewer)
        val firstId = viewer.state.value.documentId
        viewer.search("hello")
        advanceUntilIdle()

        viewer.closeDocument()
        advanceUntilIdle()

        assertEquals(1, core.documents.first().closeCount)
        assertTrue(showsHome(viewer.state.value))
        assertEquals(ViewerState(canOpen = true), viewer.state.value)
        viewer.retryPassword("stray")
        viewer.confirmClose()
        advanceUntilIdle()
        assertEquals(1, core.documents.size)
        open(viewer)
        assertNotEquals(firstId, viewer.state.value.documentId)
        assertEquals(0, core.documents.last().closeCount)
    }

    @Test
    fun cancellingClosePreservesUnsavedWork() = runTest {
        val core = CloseCore()
        val viewer = dispatchers.viewModel(core)
        open(viewer)
        edit(viewer)
        advanceUntilIdle()
        val before = viewer.state.value

        viewer.closeDocument()
        viewer.state.first { it.pendingClose != null }
        viewer.cancelClose()
        viewer.confirmClose()
        advanceUntilIdle()

        assertEquals(before, viewer.state.value)
        assertEquals(0, core.documents.first().closeCount)
    }

    @Test
    fun confirmingDiscardClosesExactlyOnce() = runTest {
        val core = CloseCore()
        val viewer = dispatchers.viewModel(core)
        open(viewer)
        edit(viewer)

        viewer.closeDocument()
        viewer.state.first { it.pendingClose != null }
        viewer.confirmClose()
        viewer.confirmClose()
        advanceUntilIdle()

        assertEquals(1, core.documents.first().closeCount)
        assertFalse(viewer.state.value.isDirty)
        assertNull(viewer.state.value.pendingClose)
        assertTrue(showsHome(viewer.state.value))
    }

    @Test
    fun consentForAnOlderRevisionAsksAgainInsteadOfDroppingNewWork() = runTest {
        val core = CloseCore()
        val viewer = dispatchers.viewModel(core)
        open(viewer)
        edit(viewer)
        viewer.closeDocument()
        viewer.state.first { it.pendingClose != null }
        val request = viewer.state.value.pendingClose
        edit(viewer)

        viewer.confirmClose()
        advanceUntilIdle()

        assertEquals(0, core.documents.first().closeCount)
        assertNotEquals(request, viewer.state.value.pendingClose)
        assertEquals(viewer.state.value.revision, viewer.state.value.pendingClose?.revision)
    }

    @Test
    fun closeChecksDirtyAgainAfterAnInFlightEdit() = runTest {
        val core = CloseCore()
        val viewer = dispatchers.viewModel(core)
        open(viewer)
        viewer.setAnnotationTool(AnnotationTool.Highlight)
        viewer.placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
        viewer.closeDocument()
        advanceUntilIdle()

        assertTrue(viewer.state.value.isDirty)
        assertNotNull(viewer.state.value.pendingClose)
        assertEquals(0, core.documents.first().closeCount)
    }

    @Test
    fun queuedSearchCannotRestoreStateAfterClose() = runTest {
        val core = CloseCore()
        val viewer = dispatchers.viewModel(core)
        open(viewer)
        viewer.closeDocument()
        viewer.search("late")
        advanceUntilIdle()

        assertEquals(0, core.documents.first().searchCount)
        assertEquals(ViewerState(canOpen = true), viewer.state.value)
    }

    @Test
    fun closeWaitsForAnAlreadyRunningSearch() = runTest {
        val core = CloseCore()
        val viewer = dispatchers.viewModel(core)
        open(viewer)
        viewer.search("hello")
        viewer.closeDocument()
        advanceUntilIdle()

        assertEquals(1, core.documents.first().searchCount)
        assertEquals(1, core.documents.first().closeCount)
        assertEquals(ViewerState(canOpen = true), viewer.state.value)
    }

    @Test
    fun viewportCallbackDuringCloseCannotScheduleAnotherRender() = runTest {
        val core = CloseCore()
        val viewer = dispatchers.viewModel(core)
        open(viewer)
        advanceUntilIdle()
        val rendered = core.documents.first().renderCount

        viewer.closeDocument()
        viewer.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1080, 1.0))
        advanceUntilIdle()

        assertEquals(rendered, core.documents.first().renderCount)
        assertEquals(ViewerState(canOpen = true), viewer.state.value)
    }

    private suspend fun open(viewer: ViewerViewModel) {
        viewer.open("test.pdf", byteArrayOf(1), saveTarget = "content://test")
        viewer.state.first { it.annotationEditingAllowed }
    }

    private suspend fun edit(viewer: ViewerViewModel) {
        val revision = viewer.state.value.revision
        viewer.setAnnotationTool(AnnotationTool.Highlight)
        viewer.placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
        viewer.state.first { it.revision > revision }
    }
}

private class CloseCore : PdfCore {
    val documents = mutableListOf<CloseDocument>()
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> =
        PdfCoreResult.Success(CloseDocument().also { documents.add(it) })
}

private class CloseDocument : PdfDocument {
    var closeCount = 0
    var searchCount = 0
    var renderCount = 0
    override val pageCount = 1
    override val pageSizes = listOf(PageSize(612.0, 792.0))
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> {
        check(closeCount == 0)
        renderCount++
        return PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    }
    override fun search(query: String): PdfCoreResult<List<SearchHit>> {
        check(closeCount == 0)
        searchCount++
        return PdfCoreResult.Success(emptyList())
    }
    override fun annotations(): PdfCoreResult<AnnotationSnapshot> =
        PdfCoreResult.Success(AnnotationSnapshot(emptyList(), editingAllowed = true, canUndo = false, canRedo = false))
    override fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> = PdfCoreResult.Success(Unit)
    override fun close() { closeCount++ }
}
