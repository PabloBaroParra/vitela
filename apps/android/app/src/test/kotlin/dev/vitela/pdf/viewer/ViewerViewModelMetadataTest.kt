package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.DocumentInfo
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Document properties: read the `/Info` dict, queue one undoable change per apply. */
class ViewerViewModelMetadataTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun openingThePropertiesLoadsTheCurrentInfo() = runTest {
        val viewModel = openedWith(MetadataDocument(DocumentInfo(title = "Report", author = "Ana")))
        viewModel.openMetadata()

        val editor = requireNotNull(viewModel.state.first { it.metadataEditor != null }.metadataEditor)
        assertEquals(DocumentInfo(title = "Report", author = "Ana"), editor.draft)
        assertTrue(editor.editingAllowed)
        assertNull(editor.message)
    }

    @Test
    fun aDocumentWithoutTheModifyBitOpensReadOnly() = runTest {
        val viewModel = openedWith(MetadataDocument(DocumentInfo(title = "Locked"), editingAllowed = false))
        viewModel.openMetadata()

        val editor = requireNotNull(viewModel.state.first { it.metadataEditor != null }.metadataEditor)
        assertFalse(editor.editingAllowed)
        assertEquals("This document does not permit metadata changes.", editor.message)
    }

    @Test
    fun applyingAnEditQueuesItAsAnUnsavedChange() = runTest {
        val document = MetadataDocument(DocumentInfo(title = "Report", author = "Ana"))
        val viewModel = openedWith(document)
        viewModel.openMetadata()
        viewModel.state.first { it.metadataEditor != null }
        viewModel.editMetadata(DocumentInfo(title = "Final report", author = ""))
        viewModel.applyMetadata()

        // Undo reaches it through the same edit log as the annotations.
        val state = viewModel.state.first { it.canUndoAnnotations }
        assertNull(state.metadataEditor)
        assertEquals(listOf(DocumentInfo(title = "Final report", author = null)), document.applied)
        assertTrue(state.isDirty)
        assertEquals(1L, state.revision)
        assertEquals("Document properties updated. Changes are pending save.", state.status)
    }

    @Test
    fun applyingAnUntouchedDraftChangesNothing() = runTest {
        val document = MetadataDocument(DocumentInfo(title = "Report"))
        val viewModel = openedWith(document)
        viewModel.openMetadata()
        viewModel.state.first { it.metadataEditor != null }
        viewModel.applyMetadata()

        val state = viewModel.state.first { it.metadataEditor == null }
        assertEquals(emptyList<DocumentInfo>(), document.applied)
        assertFalse(state.isDirty)
    }

    @Test
    fun aReadOnlyEditorNeverApplies() = runTest {
        val document = MetadataDocument(DocumentInfo(title = "Locked"), editingAllowed = false)
        val viewModel = openedWith(document)
        viewModel.openMetadata()
        viewModel.state.first { it.metadataEditor != null }
        viewModel.editMetadata(DocumentInfo(title = "Changed"))
        viewModel.applyMetadata()

        assertEquals(emptyList<DocumentInfo>(), document.applied)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun aRefusedApplyKeepsTheEditorOpenWithTheReason() = runTest {
        val document = MetadataDocument(DocumentInfo(title = "Report"), refusal = "The document could not be processed.")
        val viewModel = openedWith(document)
        viewModel.openMetadata()
        viewModel.state.first { it.metadataEditor != null }
        viewModel.editMetadata(DocumentInfo(title = "Changed"))
        viewModel.applyMetadata()

        val editor = requireNotNull(viewModel.state.first { it.metadataEditor?.message != null }.metadataEditor)
        assertEquals("The document could not be processed.", editor.message)
        assertEquals(DocumentInfo(title = "Changed"), editor.draft)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun dismissingDropsTheDraft() = runTest {
        val document = MetadataDocument(DocumentInfo(title = "Report"))
        val viewModel = openedWith(document)
        viewModel.openMetadata()
        viewModel.state.first { it.metadataEditor != null }
        viewModel.editMetadata(DocumentInfo(title = "Changed"))
        viewModel.dismissMetadata()

        assertNull(viewModel.state.value.metadataEditor)
        assertEquals(emptyList<DocumentInfo>(), document.applied)
    }

    private suspend fun openedWith(document: MetadataDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(SingleDocumentCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        return viewModel
    }
}

private class SingleDocumentCore(private val document: PdfDocument) : PdfCore {
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> = PdfCoreResult.Success(document)
}

private class MetadataDocument(
    private var info: DocumentInfo,
    private val editingAllowed: Boolean = true,
    private val refusal: String? = null,
) : PdfDocument {
    val applied = mutableListOf<DocumentInfo>()
    override val pageCount = 1
    override val pageSizes = listOf(PageSize(612.0, 792.0))
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun annotations(): PdfCoreResult<AnnotationSnapshot> =
        PdfCoreResult.Success(AnnotationSnapshot(emptyList(), editingAllowed = true, canUndo = applied.isNotEmpty(), canRedo = false))
    override fun documentInfo(): PdfCoreResult<DocumentInfo> = PdfCoreResult.Success(info)
    override fun metadataEditingAllowed(): Boolean = editingAllowed
    override fun setDocumentInfo(info: DocumentInfo): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(dev.vitela.pdf.core.PdfCoreError.Failed(it)) }
        applied += info
        this.info = info
        return PdfCoreResult.Success(Unit)
    }
    override fun close() = Unit
}
