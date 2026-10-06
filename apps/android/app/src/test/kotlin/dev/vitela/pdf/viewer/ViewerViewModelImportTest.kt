package dev.vitela.pdf.viewer

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

/**
 * Add PDFs: every picked file is prepared first, without touching the
 * document, then the whole pick goes in after the last page as ONE undo step.
 * All or nothing — a cancel or a refusal adds no page at all.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelImportTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun everyPickedPdfGoesInAfterTheLastPageInPickOrderAsOneBatch() = runTest {
        val document = OrganizableDocument(pageCount = 2)
        val (viewModel, core) = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("a.pdf", fakePdf(2)), ImportSource("b.pdf", fakePdf(1))))
        advanceUntilIdle()

        assertEquals("one batch, asked at the page count", listOf(2 to 2), document.imports)
        val state = viewModel.state.value
        assertEquals(listOf(0, 1, 2, 3, 4), state.pageSizes.map { it.widthPt.toInt() - 100 })
        assertEquals(5, state.pageCount)
        assertTrue(state.isDirty)
        assertEquals("Added 3 pages from 2 PDFs. Changes are pending save.", state.status)
        assertFalse(state.organize?.busy ?: true)
        assertTrue("every prepared source is let go", core.prepared.all { it.closed })
    }

    @Test
    fun theAddedPagesAreDrawnFromARebuiltPreview() = runTest {
        val document = OrganizableDocument(pageCount = 1)
        val (viewModel, _) = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("a.pdf", fakePdf(1))))
        advanceUntilIdle()
        viewModel.organizeThumbnail(1)
        advanceUntilIdle()

        assertEquals(listOf(1), document.drawn)
    }

    @Test
    fun oneUndoTakesTheWholePickBack() = runTest {
        val (viewModel, _) = organizing(OrganizableDocument(pageCount = 1))
        viewModel.importPdfs(listOf(ImportSource("a.pdf", fakePdf(2)), ImportSource("b.pdf", fakePdf(3))))
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(1, viewModel.state.value.pageCount)
    }

    @Test
    fun aLockedPdfAsksForItsPasswordBeforeAnythingIsAdded() = runTest {
        val document = OrganizableDocument(pageCount = 1)
        val (viewModel, core) = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("locked.pdf", fakePdf(1, FAKE_LOCKED)), ImportSource("b.pdf", fakePdf(1))))
        advanceUntilIdle()

        val organize = viewModel.state.value.organize!!
        assertEquals(ImportPasswordPrompt("locked.pdf", wrong = false), organize.importPassword)
        assertTrue("nothing else may edit while the import waits", organize.busy)
        assertTrue(document.imports.isEmpty())

        viewModel.retryImportPassword("not-it")
        advanceUntilIdle()
        assertEquals(ImportPasswordPrompt("locked.pdf", wrong = true), viewModel.state.value.organize?.importPassword)

        viewModel.retryImportPassword(FAKE_PASSWORD)
        advanceUntilIdle()
        val state = viewModel.state.value
        assertNull(state.organize?.importPassword)
        assertEquals(3, state.pageCount)
        assertEquals("the next file goes without the last one's password", 1 to null, core.preparations.last())
        assertEquals(listOf(1 to 2), document.imports)
        assertEquals("Added 2 pages from 2 PDFs. Changes are pending save.", state.status)
    }

    @Test
    fun cancellingThePasswordAddsNothing() = runTest {
        val document = OrganizableDocument(pageCount = 1)
        val (viewModel, core) = organizing(document)
        viewModel.importPdfs(
            listOf(ImportSource("a.pdf", fakePdf(1)), ImportSource("locked.pdf", fakePdf(1, FAKE_LOCKED)), ImportSource("c.pdf", fakePdf(1))),
        )
        advanceUntilIdle()
        viewModel.cancelImportPassword()
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(1, state.pageCount)
        assertEquals("Password entry cancelled for locked.pdf. No PDF was added.", state.status)
        assertFalse(state.organize?.busy ?: true)
        assertFalse(state.isDirty)
        assertTrue(document.imports.isEmpty())
        assertEquals("c.pdf is never opened", 2, core.preparations.size)
        assertTrue("a.pdf, already prepared, is let go", core.prepared.single().closed)
    }

    @Test
    fun aRefusedPdfAddsNothingWithTheCoreReason() = runTest {
        val document = OrganizableDocument(pageCount = 1)
        val (viewModel, core) = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("a.pdf", fakePdf(1)), ImportSource("refused.pdf", fakePdf(1, FAKE_REFUSED))))
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals("refused.pdf could not be added: The PDF does not permit copying its pages. No PDF was added.", state.status)
        assertEquals(1, state.pageCount)
        assertTrue(document.imports.isEmpty())
        assertFalse(state.isDirty)
        assertTrue(core.prepared.single().closed)
    }

    @Test
    fun theDocumentRefusingTheBatchAddsNothing() = runTest {
        val document = OrganizableDocument(pageCount = 1).apply { importRefusal = "This document does not allow pages to be added." }
        val (viewModel, core) = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("a.pdf", fakePdf(1)), ImportSource("b.pdf", fakePdf(1))))
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals("This document does not allow pages to be added.", state.status)
        assertEquals(1, state.pageCount)
        assertFalse(state.isDirty)
        assertFalse(state.organize?.busy ?: true)
        assertTrue(core.prepared.all { it.closed })
    }

    @Test
    fun whatThePagesWouldLoseIsAskedBeforeTheyGoIn() = runTest {
        val document = OrganizableDocument(pageCount = 1)
        val (viewModel, _) = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("form.pdf", fakePdf(1, FAKE_WARNS)), ImportSource("b.pdf", fakePdf(1))))
        advanceUntilIdle()

        val organize = viewModel.state.value.organize!!
        assertEquals(listOf("form.pdf: a form field was renamed"), organize.importWarnings)
        assertTrue(organize.busy)
        assertTrue("nothing is added before the answer", document.imports.isEmpty())

        viewModel.acceptImportWarnings()
        advanceUntilIdle()
        val state = viewModel.state.value
        assertEquals(emptyList<String>(), state.organize?.importWarnings)
        assertEquals(3, state.pageCount)
        assertEquals("Added 2 pages from 2 PDFs. Changes are pending save.", state.status)
    }

    @Test
    fun decliningTheWarningsAddsNothing() = runTest {
        val document = OrganizableDocument(pageCount = 1)
        val (viewModel, core) = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("form.pdf", fakePdf(1, FAKE_WARNS))))
        advanceUntilIdle()
        viewModel.dismissImportWarnings()
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(emptyList<String>(), state.organize?.importWarnings)
        assertFalse(state.organize?.busy ?: true)
        assertEquals("PDF import cancelled.", state.status)
        assertEquals(1, state.pageCount)
        assertTrue(document.imports.isEmpty())
        assertTrue(core.prepared.single().closed)
    }

    @Test
    fun closingTheGridWhileAQuestionIsOpenLetsThePreparedSourcesGo() = runTest {
        val document = OrganizableDocument(pageCount = 1)
        val (viewModel, core) = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("a.pdf", fakePdf(1)), ImportSource("locked.pdf", fakePdf(1, FAKE_LOCKED))))
        advanceUntilIdle()
        viewModel.closeOrganize()
        advanceUntilIdle()

        assertTrue(core.prepared.single().closed)
        assertTrue(document.imports.isEmpty())
        // A late answer from a dialog that is gone adds nothing.
        viewModel.retryImportPassword(FAKE_PASSWORD)
        advanceUntilIdle()
        assertTrue(document.imports.isEmpty())
    }

    @Test
    fun nothingIsAddedWhileTheGridIsClosed() = runTest {
        val document = OrganizableDocument()
        val (viewModel, core) = opened(document)
        viewModel.importPdfs(listOf(ImportSource("a.pdf", fakePdf(1))))
        advanceUntilIdle()

        assertTrue(core.preparations.isEmpty())
        assertTrue(document.imports.isEmpty())
    }

    @Test
    fun theSummaryCountsPagesAndFiles() {
        assertEquals("Added 1 page from 1 PDF. Changes are pending save.", importSummary(1, 1))
        assertEquals("Added 3 pages from 2 PDFs. Changes are pending save.", importSummary(3, 2))
    }

    private suspend fun TestScope.organizing(document: OrganizableDocument): Pair<ViewerViewModel, OrganizeQueueCore> =
        opened(document).also { it.first.openOrganize() }

    /** Opened and settled: the reader's first window is driven after the state is published, and would overwrite what a test sets up. */
    private suspend fun TestScope.opened(document: OrganizableDocument): Pair<ViewerViewModel, OrganizeQueueCore> {
        val core = OrganizeQueueCore(document)
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel to core
    }
}
