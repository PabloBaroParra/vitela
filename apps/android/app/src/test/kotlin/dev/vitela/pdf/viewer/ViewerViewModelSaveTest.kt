package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.AnnotationEdit
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Rule
import org.junit.Test

/**
 * Saving back to the file the user opened. The ViewModel never sees a SAF
 * `Uri`: the shell hands it an opaque save target, and the ViewModel's job is
 * to keep that target welded to the document it was opened with.
 */
class ViewerViewModelSaveTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun anOpenedFileExposesItsSaveTarget() = runTest {
        val viewModel = dispatchers.viewModel(FakeCore())
        viewModel.open("a.pdf", byteArrayOf(1), saveTarget = "content://a")
        assertEquals("content://a", viewModel.state.first { it.documentId != 0L }.saveTarget)
    }

    @Test
    fun aDocumentWithoutATargetCannotBeSavedInPlace() = runTest {
        // The built-in sample is packaged in the APK: there is nothing to write back to.
        val viewModel = dispatchers.viewModel(FakeCore())
        viewModel.open("sample.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L }
        assertNull(viewModel.state.value.saveTarget)
        assertNull(viewModel.inPlaceSave())
    }

    @Test
    fun inPlaceSavePairsTheTargetWithTheCurrentRevision() = runTest {
        val viewModel = dispatchers.viewModel(FakeCore(saved = byteArrayOf(9, 9)))
        viewModel.open("a.pdf", byteArrayOf(1), saveTarget = "content://a")
        viewModel.state.first { it.annotationEditingAllowed }
        viewModel.dirty()
        val state = viewModel.state.first { it.isDirty }

        val save = requireNotNull(viewModel.inPlaceSave())
        assertEquals("content://a", save.target)
        assertArrayEquals(byteArrayOf(9, 9), save.snapshot.bytes)
        assertEquals(state.documentId, save.snapshot.documentId)
        assertEquals(state.revision, save.snapshot.revision)

        viewModel.confirmSaved(save.snapshot)
        assertEquals(false, viewModel.state.first { !it.isDirty }.isDirty)
    }

    @Test
    fun aConfirmedReplacementCarriesItsOwnTarget() = runTest {
        val viewModel = dispatchers.viewModel(FakeCore())
        viewModel.open("a.pdf", byteArrayOf(1), saveTarget = "content://a")
        viewModel.state.first { it.annotationEditingAllowed }
        viewModel.dirty()
        viewModel.state.first { it.isDirty }

        // Parked behind the unsaved-changes prompt: A is still the open document.
        viewModel.open("b.pdf", byteArrayOf(2), saveTarget = "content://b")
        assertEquals("content://a", viewModel.state.first { it.pendingReplacementTitle != null }.saveTarget)

        viewModel.confirmReplacement()
        assertEquals("content://b", viewModel.state.first { it.title == "b.pdf" && !it.isLoading }.saveTarget)
    }

    @Test
    fun aFailedInPlaceWriteDropsTheTarget() = runTest {
        // A provider can advertise write support and still refuse the stream.
        // Leaving Save enabled would just fail again; Save copy still works.
        val viewModel = dispatchers.viewModel(FakeCore())
        viewModel.open("a.pdf", byteArrayOf(1), saveTarget = "content://a")
        viewModel.state.first { it.annotationEditingAllowed }
        viewModel.dirty()
        viewModel.state.first { it.isDirty }

        val save = requireNotNull(viewModel.inPlaceSave())
        viewModel.reportInPlaceSaveFailure(save)

        val state = viewModel.state.value
        assertNull(state.saveTarget)
        assertEquals(true, state.isDirty)
        assertEquals("Could not save to the original file. Use Save copy instead.", state.status)
    }

    @Test
    fun aStaleInPlaceFailureLeavesTheNewDocumentsTargetAlone() = runTest {
        val viewModel = dispatchers.viewModel(FakeCore())
        viewModel.open("a.pdf", byteArrayOf(1), saveTarget = "content://a")
        viewModel.state.first { it.annotationEditingAllowed }
        viewModel.dirty()
        viewModel.state.first { it.isDirty }
        val staleSave = requireNotNull(viewModel.inPlaceSave())

        viewModel.open("b.pdf", byteArrayOf(2), saveTarget = "content://b")
        viewModel.confirmReplacement()
        viewModel.state.first { it.title == "b.pdf" && !it.isLoading }

        viewModel.reportInPlaceSaveFailure(staleSave)
        assertEquals("content://b", viewModel.state.value.saveTarget)
    }

    @Test
    fun theCoreRunsOnTheInjectedDispatcherNotABackgroundThread() = runTest {
        val core = FakeCore()
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1), saveTarget = "content://a")
        viewModel.state.first { it.annotationEditingAllowed }
        viewModel.dirty()
        viewModel.state.first { it.isDirty }
        requireNotNull(viewModel.inPlaceSave())

        val testThread = Thread.currentThread()
        assertEquals(listOf("open", "annotations", "edit", "annotations", "save"), core.calls.map { it.first })
        core.calls.forEach { (call, thread) -> assertSame(call, testThread, thread) }
    }

    private fun ViewerViewModel.dirty() {
        setAnnotationTool(AnnotationTool.Highlight)
        placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
    }
}

private class FakeCore(private val saved: ByteArray = byteArrayOf(0)) : PdfCore {
    /** Each core call with the thread it ran on. */
    val calls = mutableListOf<Pair<String, Thread>>()

    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> {
        calls += "open" to Thread.currentThread()
        return PdfCoreResult.Success(FakeDocument(saved, calls))
    }
}

private class FakeDocument(private val saved: ByteArray, private val calls: MutableList<Pair<String, Thread>>) : PdfDocument {
    override val pageCount = 1
    override val pageSizes = listOf(PageSize(612.0, 792.0))
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun annotations(): PdfCoreResult<AnnotationSnapshot> {
        calls += "annotations" to Thread.currentThread()
        return PdfCoreResult.Success(AnnotationSnapshot(emptyList(), editingAllowed = true, canUndo = false, canRedo = false))
    }
    override fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> {
        calls += "edit" to Thread.currentThread()
        return PdfCoreResult.Success(Unit)
    }
    override fun saveToBytes(): PdfCoreResult<ByteArray> {
        calls += "save" to Thread.currentThread()
        return PdfCoreResult.Success(saved)
    }
    override fun close() = Unit
}
