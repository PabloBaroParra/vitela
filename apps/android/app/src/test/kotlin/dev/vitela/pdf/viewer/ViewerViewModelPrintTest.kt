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
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Rule
import org.junit.Test

/**
 * Printing rasterizes the core's output snapshot, not the live document: the
 * live render preview leaves the session's annotations out (the shell overlays
 * them), so printing from it would drop them. The core reopens the snapshot
 * itself, under the password it already holds, so an encrypted document
 * prints without this shell ever keeping one.
 */
class ViewerViewModelPrintTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun printsTheCoresOutputSnapshotNotTheLiveDocument() = runTest {
        val core = PrintCore()
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.canPrint }

        val printed = viewModel.printDocument()

        val live = core.opened.single()
        assertSame(live.snapshots.single(), printed)
        assertFalse("the live document must stay open", live.closed)
        assertFalse(live.snapshots.single().closed)
    }

    @Test
    fun printingNeedsNoPasswordFromTheShell() = runTest {
        val core = PrintCore()
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.canPrint }

        assertNotNull(viewModel.printDocument())

        assertEquals("only the live document is ever opened by the shell", 1, core.opened.size)
        assertEquals(listOf<String?>(null), core.openedPasswords)
    }

    @Test
    fun aSnapshotThatFailsReportsWhy() = runTest {
        val core = PrintCore(snapshotError = PdfCoreError.Failed("boom"))
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.canPrint }

        assertNull(viewModel.printDocument())

        assertEquals("boom", viewModel.state.value.status)
        assertFalse("the live document must stay open", core.opened.single().closed)
    }

    @Test
    fun aFailedSnapshotIsNotReplacedByTheStaleSourceFile() = runTest {
        // The source bytes predate every edit: printing them would silently drop the user's changes.
        val core = PrintCore(snapshotSupported = false)
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.canPrint }

        assertNull(viewModel.printDocument())

        assertEquals("Printing is unavailable in this PDF core.", viewModel.state.value.status)
        assertEquals(1, core.opened.size)
    }

    @Test
    fun withNothingOpenThereIsNothingToPrint() = runTest {
        val viewModel = dispatchers.viewModel(PrintCore())
        assertNull(viewModel.printDocument())
    }
}

private class PrintCore(
    private val snapshotError: PdfCoreError? = null,
    private val snapshotSupported: Boolean = true,
) : PdfCore {
    val opened = mutableListOf<PrintDocument>()
    val openedPasswords = mutableListOf<String?>()

    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> {
        openedPasswords += password
        return PdfCoreResult.Success(PrintDocument(snapshotError, snapshotSupported).also { opened += it })
    }
}

private class PrintDocument(
    private val snapshotError: PdfCoreError?,
    private val snapshotSupported: Boolean,
) : PdfDocument {
    var closed = false
    val snapshots = mutableListOf<PrintDocument>()
    override val pageCount = 1
    override val pageSizes = listOf(PageSize(612.0, 792.0))
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun annotations(): PdfCoreResult<AnnotationSnapshot> =
        PdfCoreResult.Success(AnnotationSnapshot(emptyList(), editingAllowed = true, canUndo = false, canRedo = false))
    override fun outputSnapshot(): PdfCoreResult<PdfDocument> = when {
        !snapshotSupported -> super.outputSnapshot()
        snapshotError != null -> PdfCoreResult.Failure(snapshotError)
        else -> PdfCoreResult.Success(PrintDocument(null, true).also { snapshots += it })
    }
    override fun close() {
        closed = true
    }
}
