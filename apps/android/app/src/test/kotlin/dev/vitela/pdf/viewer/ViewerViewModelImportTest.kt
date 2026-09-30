package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Add PDFs: other files' pages go in after the last page, one undo step per file. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelImportTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun everyPickedPdfGoesInAfterTheLastPageInPickOrder() = runTest {
        val document = OrganizableDocument(pageCount = 2)
        val viewModel = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("a.pdf", fakePdf(2)), ImportSource("b.pdf", fakePdf(1))))
        advanceUntilIdle()

        assertEquals("each file is asked at the page count it found", listOf(2 to null, 4 to null), document.imports)
        val state = viewModel.state.value
        assertEquals(listOf(0, 1, 2, 3, 4), state.pageSizes.map { it.widthPt.toInt() - 100 })
        assertEquals(5, state.pageCount)
        assertTrue(state.isDirty)
        assertEquals("Added 3 pages from 2 PDFs. Changes are pending save.", state.status)
        assertFalse(state.organize?.busy ?: true)
    }

    @Test
    fun theAddedPagesAreDrawnFromARebuiltPreview() = runTest {
        val document = OrganizableDocument(pageCount = 1)
        val viewModel = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("a.pdf", fakePdf(1))))
        advanceUntilIdle()
        viewModel.organizeThumbnail(1)
        advanceUntilIdle()

        assertEquals(listOf(1), document.drawn)
    }

    @Test
    fun oneUndoTakesOneFileBack() = runTest {
        val document = OrganizableDocument(pageCount = 1)
        val viewModel = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("a.pdf", fakePdf(2)), ImportSource("b.pdf", fakePdf(3))))
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(3, viewModel.state.value.pageCount)
    }

    @Test
    fun aLockedPdfAsksForItsPasswordAndTheRestWaitForIt() = runTest {
        val document = OrganizableDocument(pageCount = 1)
        val viewModel = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("locked.pdf", fakePdf(1, FAKE_LOCKED)), ImportSource("b.pdf", fakePdf(1))))
        advanceUntilIdle()

        val organize = viewModel.state.value.organize!!
        assertEquals(ImportPasswordPrompt("locked.pdf", wrong = false), organize.importPassword)
        assertTrue("nothing else may edit while the import waits", organize.busy)
        assertEquals(listOf(1 to null), document.imports)

        viewModel.retryImportPassword("not-it")
        advanceUntilIdle()
        assertEquals(ImportPasswordPrompt("locked.pdf", wrong = true), viewModel.state.value.organize?.importPassword)

        viewModel.retryImportPassword(FAKE_PASSWORD)
        advanceUntilIdle()
        val state = viewModel.state.value
        assertNull(state.organize?.importPassword)
        assertEquals(3, state.pageCount)
        assertEquals("the next file goes without the last one's password", 2 to null, document.imports.last())
        assertEquals("Added 2 pages from 2 PDFs. Changes are pending save.", state.status)
    }

    @Test
    fun cancellingThePasswordStopsTheRestButKeepsWhatWentIn() = runTest {
        val document = OrganizableDocument(pageCount = 1)
        val viewModel = organizing(document)
        viewModel.importPdfs(
            listOf(ImportSource("a.pdf", fakePdf(1)), ImportSource("locked.pdf", fakePdf(1, FAKE_LOCKED)), ImportSource("c.pdf", fakePdf(1))),
        )
        advanceUntilIdle()
        viewModel.cancelImportPassword()
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(2, state.pageCount)
        assertEquals("Added 1 page from 1 PDF. Changes are pending save. Password entry cancelled for locked.pdf.", state.status)
        assertFalse(state.organize?.busy ?: true)
        assertEquals(2, document.imports.size)
    }

    @Test
    fun aRefusedPdfStopsTheRestWithTheCoreReason() = runTest {
        val document = OrganizableDocument(pageCount = 1)
        val viewModel = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("refused.pdf", fakePdf(1, FAKE_REFUSED)), ImportSource("b.pdf", fakePdf(1))))
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals("refused.pdf could not be added: The PDF does not permit copying its pages.", state.status)
        assertEquals(1, state.pageCount)
        assertEquals(1, document.imports.size)
        assertFalse(state.isDirty)
    }

    @Test
    fun whatAFileLostIsShownUntilDismissed() = runTest {
        val viewModel = organizing(OrganizableDocument(pageCount = 1))
        viewModel.importPdfs(listOf(ImportSource("form.pdf", fakePdf(1, FAKE_WARNS))))
        advanceUntilIdle()

        assertEquals(listOf("form.pdf: a form field was renamed"), viewModel.state.value.organize?.importWarnings)
        viewModel.dismissImportWarnings()
        assertEquals(emptyList<String>(), viewModel.state.value.organize?.importWarnings)
    }

    @Test
    fun nothingIsAddedWhileTheGridIsClosed() = runTest {
        val document = OrganizableDocument()
        val viewModel = opened(document)
        viewModel.importPdfs(listOf(ImportSource("a.pdf", fakePdf(1))))
        advanceUntilIdle()

        assertTrue(document.imports.isEmpty())
    }

    @Test
    fun theSummaryCountsPagesAndFiles() {
        assertEquals("Added 1 page from 1 PDF. Changes are pending save.", importSummary(1, 1, null))
        assertEquals("No PDF was added.", importSummary(0, 0, null))
        assertEquals("x", importSummary(0, 0, "x"))
    }

    private suspend fun TestScope.organizing(document: PdfDocument): ViewerViewModel =
        opened(document).also { it.openOrganize() }

    /** Opened and settled: the reader's first window is driven after the state is published, and would overwrite what a test sets up. */
    private suspend fun TestScope.opened(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
