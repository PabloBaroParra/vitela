package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.BlockSource
import dev.vitela.pdf.core.DocumentBlock
import dev.vitela.pdf.core.PageEdit
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

/** Organize > Documents: one card per run of pages from the same PDF, each moved, turned or deleted whole. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelOrganizeDocumentsTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun theGridOpensOnPagesAndSwitchesToDocuments() = runTest {
        val viewModel = organizing(OrganizableDocument(pageCount = 3))
        assertEquals(OrganizeView.Pages, viewModel.state.value.organize?.view)

        viewModel.organizeShow(OrganizeView.Documents)
        advanceUntilIdle()

        val organize = viewModel.state.value.organize!!
        assertEquals(OrganizeView.Documents, organize.view)
        assertEquals(listOf(DocumentBlock(BlockSource.Base, null, 0, 3)), organize.blocks)
    }

    @Test
    fun anAddedPdfIsItsOwnBlockKnownByItsFileName() = runTest {
        val viewModel = organizing(OrganizableDocument(pageCount = 2))
        viewModel.importPdfs(listOf(ImportSource("added.pdf", fakePdf(3))))
        advanceUntilIdle()
        viewModel.organizeShow(OrganizeView.Documents)
        advanceUntilIdle()

        val state = viewModel.state.value
        val blocks = state.organize!!.blocks
        assertEquals(2, blocks.size)
        assertEquals(listOf("a.pdf", "added.pdf"), blocks.map { blockTitle(it, state.title, state.importedSourceNames) })
    }

    @Test
    fun movingABlockMovesAllOfItsPagesAsOneEdit() = runTest {
        val document = OrganizableDocument(pageCount = 2)
        val viewModel = documents(document, added = 2)
        val addedBlock = viewModel.state.value.organize!!.blocks[1]

        viewModel.organizeMoveBlock(addedBlock, -1)
        advanceUntilIdle()

        assertEquals(PageEdit.Move(from = 2, to = 0, count = 2), document.edits.last())
        val state = viewModel.state.value
        assertEquals(listOf(2, 3, 0, 1), state.pageSizes.map { it.widthPt.toInt() - 100 })
        assertEquals(listOf(BlockSource.Imported(40), BlockSource.Base), state.organize!!.blocks.map { it.source })
        assertEquals("2 pages moved. Changes are pending save.", state.status)
    }

    @Test
    fun turningABlockTurnsEveryPageOfIt() = runTest {
        val document = OrganizableDocument(pageCount = 2)
        val viewModel = documents(document, added = 2)

        viewModel.organizeRotateBlock(viewModel.state.value.organize!!.blocks[1], 90)
        advanceUntilIdle()

        assertEquals(PageEdit.Rotate(2, 90, count = 2), document.edits.last())
        assertEquals(listOf(false, false, true, true), viewModel.state.value.pageSizes.map { it.widthPt > it.heightPt })
    }

    @Test
    fun deletingABlockIsOneUndoStep() = runTest {
        val document = OrganizableDocument(pageCount = 2)
        val viewModel = documents(document, added = 3)

        viewModel.organizeDeleteBlock(viewModel.state.value.organize!!.blocks[1])
        advanceUntilIdle()
        assertEquals(PageEdit.Remove(2, count = 3), document.edits.last())
        assertEquals(1, viewModel.state.value.organize!!.blocks.size)

        viewModel.undoAnnotations()
        advanceUntilIdle()
        assertEquals(5, viewModel.state.value.pageCount)
        assertEquals("an undo re-reads the blocks too", 2, viewModel.state.value.organize!!.blocks.size)
    }

    @Test
    fun theOnlyBlockCannotBeDeleted() = runTest {
        val document = OrganizableDocument(pageCount = 3)
        val viewModel = organizing(document)
        viewModel.organizeShow(OrganizeView.Documents)
        advanceUntilIdle()

        viewModel.organizeDeleteBlock(viewModel.state.value.organize!!.blocks.single())
        advanceUntilIdle()

        assertTrue(document.edits.isEmpty())
        assertEquals(ORGANIZE_LAST_PAGE, viewModel.state.value.status)
    }

    @Test
    fun aBlockThatMovedSinceItWasShownIsNotEdited() = runTest {
        val document = OrganizableDocument(pageCount = 2)
        val viewModel = documents(document, added = 2)
        val stale = viewModel.state.value.organize!!.blocks[1].copy(start = 1)

        viewModel.organizeDeleteBlock(stale)
        advanceUntilIdle()

        assertTrue(document.edits.isEmpty())
        assertEquals(ORGANIZE_NO_SUCH_PAGE, viewModel.state.value.status)
    }

    @Test
    fun anotherDocumentForgetsTheAddedFilesNames() = runTest {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(OrganizableDocument(pageCount = 2), OrganizableDocument(pageCount = 2)))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        viewModel.openOrganize()
        viewModel.importPdfs(listOf(ImportSource("added.pdf", fakePdf(1))))
        advanceUntilIdle()
        assertEquals(mapOf(40L to "added.pdf"), viewModel.state.value.importedSourceNames)

        viewModel.open("b.pdf", byteArrayOf(1))
        // The added pages are unsaved, so replacing the document asks first.
        viewModel.confirmReplacement()
        viewModel.state.first { it.title == "b.pdf" && !it.isLoading }
        advanceUntilIdle()

        assertEquals(emptyMap<Long, String>(), viewModel.state.value.importedSourceNames)
    }

    /** Organizing on the Documents view, with [added] pages of `added.pdf` after the document's own. */
    private suspend fun TestScope.documents(document: OrganizableDocument, added: Int): ViewerViewModel {
        val viewModel = organizing(document)
        viewModel.importPdfs(listOf(ImportSource("added.pdf", fakePdf(added))))
        advanceUntilIdle()
        viewModel.organizeShow(OrganizeView.Documents)
        advanceUntilIdle()
        return viewModel
    }

    private suspend fun TestScope.organizing(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        viewModel.openOrganize()
        return viewModel
    }
}
