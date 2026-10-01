package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test

/**
 * Printing rasterizes a throwaway copy of the document's saved snapshot, not
 * the live document: the live render preview leaves the session's annotations
 * out (the shell overlays them), so printing from it would drop them.
 */
class ViewerViewModelPrintTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun printsASeparateDocumentOpenedFromTheSavedSnapshot() = runTest {
        val core = PrintCore(snapshot = byteArrayOf(7, 7, 7))
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.canPrint }

        val printed = viewModel.printDocument()

        assertNotNull(printed)
        assertNotSame(core.opened.first(), printed)
        assertEquals(2, core.opened.size)
        assertArrayEquals(byteArrayOf(7, 7, 7), core.openedBytes.last())
        assertNull(core.openedPasswords.last())
        assertFalse("the live document must stay open", core.opened.first().closed)
        assertFalse(core.opened.last().closed)
    }

    @Test
    fun aSnapshotThatNeedsAPasswordIsNeverPrinted() = runTest {
        val core = PrintCore(snapshot = byteArrayOf(7), snapshotOpenError = PdfCoreError.PasswordRequired)
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.canPrint }

        assertNull(viewModel.printDocument())

        assertEquals("Printing a password-protected document is not supported yet.", viewModel.state.value.status)
        assertFalse("the live document must stay open", core.opened.first().closed)
    }

    @Test
    fun aSnapshotThatFailsToOpenReportsWhy() = runTest {
        val core = PrintCore(snapshot = byteArrayOf(7), snapshotOpenError = PdfCoreError.Failed("boom"))
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.canPrint }

        assertNull(viewModel.printDocument())

        assertEquals("boom", viewModel.state.value.status)
    }

    @Test
    fun aFailedSnapshotIsNotReplacedByTheStaleSourceFile() = runTest {
        // The source bytes predate every edit: printing them would silently drop the user's changes.
        val core = PrintCore(snapshot = null)
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.canPrint }

        assertNull(viewModel.printDocument())

        assertEquals("Saving is unavailable in this PDF core.", viewModel.state.value.status)
        assertEquals(1, core.opened.size)
    }

    @Test
    fun withNothingOpenThereIsNothingToPrint() = runTest {
        val viewModel = dispatchers.viewModel(PrintCore(snapshot = byteArrayOf(1)))
        assertNull(viewModel.printDocument())
    }
}

private class PrintCore(
    private val snapshot: ByteArray?,
    private val snapshotOpenError: PdfCoreError? = null,
) : PdfCore {
    val opened = mutableListOf<PrintDocument>()
    val openedBytes = mutableListOf<ByteArray>()
    val openedPasswords = mutableListOf<String?>()

    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> {
        // The first open is the live document; any later one is a print snapshot.
        if (opened.isNotEmpty() && snapshotOpenError != null) return PdfCoreResult.Failure(snapshotOpenError)
        openedBytes += bytes
        openedPasswords += password
        return PdfCoreResult.Success(PrintDocument(snapshot).also { opened += it })
    }
}

private class PrintDocument(private val snapshot: ByteArray?) : PdfDocument {
    var closed = false
    override val pageCount = 1
    override val pageSizes = listOf(PageSize(612.0, 792.0))
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun annotations(): PdfCoreResult<AnnotationSnapshot> =
        PdfCoreResult.Success(AnnotationSnapshot(emptyList(), editingAllowed = true, canUndo = false, canRedo = false))
    override fun saveToBytes(): PdfCoreResult<ByteArray> =
        if (snapshot == null) super.saveToBytes() else PdfCoreResult.Success(snapshot)
    override fun close() {
        closed = true
    }
}
