package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PageEdit
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.SearchHit
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Organize: a grid of pages standing in for the reader, each change one undoable edit of the shared log. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelOrganizeTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun openingOrganizeShowsTheGridAndClearsWhatPointsAtTheReader() = runTest {
        val viewModel = openedWith(OrganizableDocument())
        viewModel.openOrganize()

        assertNotNull(viewModel.state.value.organize)
        assertNull(viewModel.state.value.selectedAnnotationId)
        assertNull(viewModel.state.value.textSelection)
        assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
    }

    @Test
    fun leavingOrganizeReturnsTheReaderToTheCurrentPage() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 5))
        viewModel.onReaderPositionChanged(ReaderPosition(2, 3, 3, 0, 1.0))
        viewModel.openOrganize()
        viewModel.closeOrganize()

        val state = viewModel.state.value
        assertNull(state.organize)
        assertEquals(3, state.scrollTarget)
    }

    @Test
    fun aMoveReachesTheCoreAndRelaysTheReader() = runTest {
        val document = OrganizableDocument(pageCount = 4)
        val viewModel = openedWith(document)
        viewModel.openOrganize()
        viewModel.organizeMove(index = 0, delta = 1)
        advanceUntilIdle()

        assertEquals(listOf<PageEdit>(PageEdit.Move(0, 1)), document.edits)
        val state = viewModel.state.value
        assertEquals(listOf(1, 0, 2, 3), state.pageSizes.map { it.widthPt.toInt() - 100 })
        assertEquals(4, state.pageCount)
        assertEquals("Page moved. Changes are pending save.", state.status)
        assertFalse(state.organize?.busy ?: true)
    }

    @Test
    fun anEditMakesTheDocumentDirtyAndLightsUpUndo() = runTest {
        val viewModel = openedWith(OrganizableDocument())
        val before = viewModel.state.value
        viewModel.openOrganize()
        viewModel.organizeMove(0, 1)
        advanceUntilIdle()

        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertEquals(before.revision + 1, state.revision)
        assertTrue(state.canUndoAnnotations)
        assertFalse(state.canRedoAnnotations)
    }

    @Test
    fun aQuarterTurnSwapsThePagesSidesInTheLayout() = runTest {
        val document = OrganizableDocument()
        val viewModel = openedWith(document)
        viewModel.openOrganize()
        viewModel.organizeRotate(index = 1, delta = 90)
        advanceUntilIdle()

        assertEquals(listOf<PageEdit>(PageEdit.Rotate(1, 90)), document.edits)
        assertEquals(PageSize(200.0, 101.0), viewModel.state.value.pageSizes[1])
        assertEquals("Page rotated. Changes are pending save.", viewModel.state.value.status)
    }

    @Test
    fun deletingAPageShrinksTheDocumentAndKeepsThePositionInRange() = runTest {
        val document = OrganizableDocument(pageCount = 3)
        val viewModel = openedWith(document)
        viewModel.onReaderPositionChanged(ReaderPosition(2, 2, 2, 0, 1.0))
        viewModel.openOrganize()
        viewModel.organizeDelete(2)
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(2, state.pageCount)
        assertEquals(2, state.pageSizes.size)
        assertEquals(1, state.pageIndex)
        assertEquals("Page deleted. Changes are pending save.", state.status)
    }

    @Test
    fun theLastPageIsNeverDeletedAndTheCoreIsNotAsked() = runTest {
        val document = OrganizableDocument(pageCount = 1)
        val viewModel = openedWith(document)
        viewModel.openOrganize()
        viewModel.organizeDelete(0)
        advanceUntilIdle()

        assertEquals(emptyList<PageEdit>(), document.edits)
        assertEquals(ORGANIZE_LAST_PAGE, viewModel.state.value.status)
        assertFalse(viewModel.state.value.isDirty)
        assertEquals(1, viewModel.state.value.pageCount)
    }

    @Test
    fun aRefusalFromTheCoreChangesNothingAndSaysWhy() = runTest {
        val refusal = "This document does not allow its pages to be rearranged, rotated or removed."
        val document = OrganizableDocument(refusal = PdfCoreError.Failed(refusal))
        val viewModel = openedWith(document)
        val before = viewModel.state.value
        viewModel.openOrganize()
        viewModel.organizeMove(0, 1)
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(refusal, state.status)
        assertFalse(state.isDirty)
        assertEquals(before.revision, state.revision)
        assertEquals(before.pageSizes, state.pageSizes)
        assertNotNull("the grid stays open", state.organize)
        assertFalse(state.organize?.busy ?: true)
    }

    @Test
    fun anEditOutsideTheDocumentNeverReachesTheCore() = runTest {
        val document = OrganizableDocument(pageCount = 2)
        val viewModel = openedWith(document)
        viewModel.openOrganize()
        viewModel.organizeMove(index = 1, delta = 1)
        viewModel.organizeRotate(index = 7, delta = 90)
        advanceUntilIdle()

        assertEquals(emptyList<PageEdit>(), document.edits)
    }

    @Test
    fun editsAreIgnoredWhileTheGridIsClosed() = runTest {
        val document = OrganizableDocument()
        val viewModel = openedWith(document)
        viewModel.organizeMove(0, 1)
        viewModel.organizeRotate(0, 90)
        viewModel.organizeDelete(0)
        advanceUntilIdle()

        assertEquals(emptyList<PageEdit>(), document.edits)
    }

    @Test
    fun anEditForgetsWhatWasCachedByPagePosition() = runTest {
        val document = OrganizableDocument()
        val viewModel = openedWith(document)
        viewModel.search("x")
        viewModel.state.first { it.searchHits.isNotEmpty() }
        viewModel.openOrganize()
        viewModel.organizeMove(0, 1)
        advanceUntilIdle()

        assertEquals(emptyList<SearchHit>(), viewModel.state.value.searchHits)
        assertEquals(0, viewModel.state.value.searchIndex)
    }

    @Test
    fun undoingAPageEditRestoresTheLayout() = runTest {
        val document = OrganizableDocument(pageCount = 4)
        val viewModel = openedWith(document)
        val original = viewModel.state.value.pageSizes
        viewModel.openOrganize()
        viewModel.organizeDelete(1)
        advanceUntilIdle()
        assertEquals(3, viewModel.state.value.pageCount)

        viewModel.undoAnnotations()
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(4, state.pageCount)
        assertEquals(original, state.pageSizes)
        assertTrue(state.canRedoAnnotations)
        assertFalse(state.canUndoAnnotations)
    }

    @Test
    fun redoingAPageEditReappliesTheLayout() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 4))
        viewModel.openOrganize()
        viewModel.organizeDelete(1)
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()
        viewModel.redoAnnotations()
        advanceUntilIdle()

        assertEquals(3, viewModel.state.value.pageCount)
    }

    @Test
    fun aSessionThatNeverEditedItsPagesDoesNotReReadThemOnUndo() = runTest {
        val document = OrganizableDocument()
        val viewModel = openedWith(document)
        document.stackAnAnnotationUndo()
        val reads = document.pageSizeReads
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(reads, document.pageSizeReads)
    }

    @Test
    fun theThumbnailsOfAnOldLayoutAreNeverKeptForANewOne() = runTest {
        val viewModel = openedWith(OrganizableDocument())
        viewModel.openOrganize()
        val opened = requireNotNull(viewModel.state.value.organize).version
        viewModel.organizeMove(0, 1)
        advanceUntilIdle()

        assertTrue(requireNotNull(viewModel.state.value.organize).version > opened)
    }

    @Test
    fun aThumbnailIsRequestedAtACheapDensityForTheGivenPosition() = runTest {
        val document = OrganizableDocument()
        val viewModel = openedWith(document)
        viewModel.openOrganize()
        viewModel.organizeThumbnail(2)
        advanceUntilIdle()

        assertEquals(listOf(2 to thumbnailDpi(PageSize(102.0, 200.0), THUMBNAIL_BOX_PX)), document.rendered)
    }

    @Test
    fun aThumbnailIsNotRequestedWhileTheGridIsClosed() = runTest {
        val document = OrganizableDocument()
        val viewModel = openedWith(document)
        viewModel.organizeThumbnail(0)
        advanceUntilIdle()

        assertEquals(emptyList<Pair<Int, Int>>(), document.rendered)
    }

    @Test
    fun replacingTheDocumentClosesTheGrid() = runTest {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(OrganizableDocument(), OrganizableDocument()))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId == 1L && !it.isLoading }
        viewModel.openOrganize()
        viewModel.open("b.pdf", byteArrayOf(2))
        // a is clean, so b replaces it without a prompt.
        viewModel.state.first { it.documentId == 2L && !it.isLoading }
        advanceUntilIdle()

        assertNull(viewModel.state.value.organize)
    }

    /** Opened and settled: the reader's first window is driven after the state is published, and would overwrite what a test sets up. */
    private suspend fun TestScope.openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
