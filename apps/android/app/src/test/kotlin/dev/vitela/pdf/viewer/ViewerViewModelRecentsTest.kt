package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationEdit
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit
import dev.vitela.pdf.home.RecentDocument
import dev.vitela.pdf.home.RecentStore
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Home's Recent list: a document opened from a reopenable source goes first;
 * a card the user removes stays removed — across restarts — until the
 * document is opened again.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelRecentsTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    private val store = MemoryRecentStore()
    private var now = 1_000L
    private val previews = mutableListOf<RenderedPage>()

    private fun viewModel() = dispatchers.viewModel(
        RecentsCore(),
        recents = store,
        encodePreview = { page -> previews += page; byteArrayOf(9, 9) },
        clock = { now },
    )

    @Test
    fun aReopenableDocumentGoesFirstWithItsPageCountAndPreview() = runTest {
        val viewModel = viewModel()

        viewModel.open("a.pdf", PLAIN, recent = "content://a")
        advanceUntilIdle()

        assertEquals(listOf(RecentDocument("content://a", "a.pdf", 1_000L, 2)), viewModel.recentDocuments.value)
        assertEquals(viewModel.recentDocuments.value, store.entries)
        assertArrayEquals(byteArrayOf(9, 9), store.thumbnails["content://a"])
        assertEquals(1, previews.size)
    }

    @Test
    fun aDocumentWithNoReopenableSourceIsNotRemembered() = runTest {
        val viewModel = viewModel()

        viewModel.open("sample.pdf", PLAIN)
        advanceUntilIdle()

        assertEquals(emptyList<RecentDocument>(), viewModel.recentDocuments.value)
        assertTrue(previews.isEmpty())
    }

    @Test
    fun aDocumentThatFailsToOpenIsNotRemembered() = runTest {
        val viewModel = viewModel()

        viewModel.open("locked.pdf", LOCKED, recent = "content://locked")
        viewModel.state.first { it.needsPassword }
        viewModel.cancelPassword()
        advanceUntilIdle()

        assertEquals(emptyList<RecentDocument>(), viewModel.recentDocuments.value)
    }

    /** Its first page must never be written to disk: the file is protected for a reason. */
    @Test
    fun aDocumentOpenedWithAPasswordIsRememberedWithoutAPreview() = runTest {
        val viewModel = viewModel()
        store.thumbnails["content://locked"] = byteArrayOf(1)

        viewModel.open("locked.pdf", LOCKED, recent = "content://locked")
        viewModel.state.first { it.needsPassword }
        viewModel.retryPassword(PASSWORD)
        advanceUntilIdle()

        assertEquals(listOf("content://locked"), viewModel.recentDocuments.value.map { it.uri })
        assertTrue(previews.isEmpty())
        assertNull("an older preview of it goes too", store.thumbnails["content://locked"])
    }

    @Test
    fun aRemovedCardStaysRemovedAfterARestart() = runTest {
        val first = viewModel()
        first.open("a.pdf", PLAIN, recent = "content://a")
        advanceUntilIdle()
        first.open("b.pdf", PLAIN, recent = "content://b")
        advanceUntilIdle()

        first.removeRecent("content://a")
        advanceUntilIdle()

        assertEquals(listOf("content://b"), first.recentDocuments.value.map { it.uri })
        assertNull(store.thumbnails["content://a"])
        val restarted = viewModel()
        restarted.loadRecents()
        advanceUntilIdle()
        assertEquals(listOf("content://b"), restarted.recentDocuments.value.map { it.uri })
    }

    @Test
    fun openingARemovedDocumentAgainBringsItBack() = runTest {
        val viewModel = viewModel()
        viewModel.open("a.pdf", PLAIN, recent = "content://a")
        advanceUntilIdle()
        viewModel.removeRecent("content://a")
        advanceUntilIdle()

        now = 2_000L
        viewModel.open("a.pdf", PLAIN, recent = "content://a")
        advanceUntilIdle()

        assertEquals(listOf(RecentDocument("content://a", "a.pdf", 2_000L, 2)), viewModel.recentDocuments.value)
    }

    /** The first open after launch can land before Home asked for the list; it must add to it, not replace it. */
    @Test
    fun anOpenBeforeTheListLoadedKeepsWhatWasStored() = runTest {
        store.entries = listOf(RecentDocument("content://old", "old.pdf", 5L, 1))
        val viewModel = viewModel()

        viewModel.open("a.pdf", PLAIN, recent = "content://a")
        advanceUntilIdle()

        assertEquals(listOf("content://a", "content://old"), viewModel.recentDocuments.value.map { it.uri })
    }

    @Test
    fun aReplacementConfirmedOverUnsavedChangesIsRemembered() = runTest {
        val viewModel = viewModel()
        viewModel.open("a.pdf", PLAIN)
        viewModel.state.first { it.annotationEditingAllowed }
        viewModel.setAnnotationTool(AnnotationTool.Highlight)
        viewModel.placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
        viewModel.state.first { it.isDirty }

        viewModel.open("b.pdf", PLAIN, recent = "content://b")
        viewModel.state.first { it.pendingReplacementTitle == "b.pdf" }
        viewModel.confirmReplacement()
        advanceUntilIdle()

        assertEquals(listOf("content://b"), viewModel.recentDocuments.value.map { it.uri })
    }

    private companion object {
        val PLAIN = byteArrayOf(1)
        val LOCKED = byteArrayOf(2)
        const val PASSWORD = "secret"
    }
}

internal class MemoryRecentStore : RecentStore {
    var entries: List<RecentDocument> = emptyList()
    val thumbnails = mutableMapOf<String, ByteArray>()

    override fun load(): List<RecentDocument> = entries
    override fun save(entries: List<RecentDocument>): Boolean {
        this.entries = entries
        return true
    }
    override fun thumbnail(uri: String): ByteArray? = thumbnails[uri]
    override fun saveThumbnail(uri: String, png: ByteArray): Boolean {
        thumbnails[uri] = png
        return true
    }
    override fun deleteThumbnail(uri: String) {
        thumbnails.remove(uri)
    }
}

/** Opens anything, except locked bytes without the right password. */
private class RecentsCore : PdfCore {
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> = when {
        bytes.contentEquals(byteArrayOf(2)) && password == null -> PdfCoreResult.Failure(PdfCoreError.PasswordRequired)
        bytes.contentEquals(byteArrayOf(2)) && password != "secret" -> PdfCoreResult.Failure(PdfCoreError.WrongPassword)
        else -> PdfCoreResult.Success(RecentsDocument())
    }
}

private class RecentsDocument : PdfDocument {
    override val pageCount = 2
    override val pageSizes = listOf(PageSize(612.0, 792.0), PageSize(612.0, 792.0))
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun annotations(): PdfCoreResult<AnnotationSnapshot> =
        PdfCoreResult.Success(AnnotationSnapshot(emptyList(), editingAllowed = true, canUndo = false, canRedo = false))
    override fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> = PdfCoreResult.Success(Unit)
    override fun close() = Unit
}
