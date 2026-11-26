package dev.vitela.pdf.viewer

import dev.vitela.pdf.R
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
import dev.vitela.pdf.text.UiText
import dev.vitela.pdf.text.english
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
 * Share serializes the session exactly as a save would — pending edits
 * included — without that copy counting as this session saved, as the
 * Windows shell's `ShareBytesAsync` does.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelShareTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun aSharedCopyCarriesThePendingEditsAndLeavesTheSessionUnsaved() = runTest {
        val viewModel = dispatchers.viewModel(ShareCore(PdfCoreResult.Success(byteArrayOf(7, 7))))
        viewModel.open("a.pdf", byteArrayOf(1), saveTarget = "content://a")
        viewModel.state.first { it.annotationEditingAllowed }
        viewModel.dirty()
        viewModel.state.first { it.isDirty }

        val shared = requireNotNull(viewModel.shareBytes(viewModel.state.value.documentId))
        advanceUntilIdle()

        assertArrayEquals(byteArrayOf(7, 7), shared.bytes)
        assertTrue("a copy sent elsewhere is not this session saved", viewModel.state.value.isDirty)
    }

    @Test
    fun aCleanDocumentCanBeSharedToo() = runTest {
        val viewModel = dispatchers.viewModel(ShareCore(PdfCoreResult.Success(byteArrayOf(5))))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }

        assertArrayEquals(byteArrayOf(5), requireNotNull(viewModel.shareBytes(viewModel.state.value.documentId)).bytes)
    }

    @Test
    fun theSharedCopyIsNamedAfterTheDocumentThatProducedItsBytes() = runTest {
        val viewModel = dispatchers.viewModel(ShareCore(PdfCoreResult.Success(byteArrayOf(5))))
        viewModel.open("Contract.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }

        val shared = requireNotNull(viewModel.shareBytes(viewModel.state.value.documentId))

        assertEquals("Contract.pdf", shared.name)
        assertEquals(viewModel.state.value.documentId, shared.documentId)
    }

    @Test
    fun aShareTappedForAReplacedDocumentSharesNothingAndSaysNothing() = runTest {
        val viewModel = dispatchers.viewModel(ShareCore(PdfCoreResult.Success(byteArrayOf(5))))
        viewModel.open("a.pdf", byteArrayOf(1))
        val tapped = viewModel.state.first { it.documentId != 0L && !it.isLoading }.documentId
        viewModel.open("b.pdf", byteArrayOf(2))
        val replaced = viewModel.state.first { it.documentId != tapped && it.documentId != 0L && !it.isLoading }

        assertNull(viewModel.shareBytes(tapped))
        assertEquals(replaced.status, viewModel.state.value.status)
    }

    @Test
    fun aLateShareFailureFromAReplacedDocumentIsDropped() = runTest {
        val viewModel = dispatchers.viewModel(ShareCore(PdfCoreResult.Success(byteArrayOf(5))))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        val shared = requireNotNull(viewModel.shareBytes(viewModel.state.value.documentId))
        viewModel.open("b.pdf", byteArrayOf(2))
        val replaced = viewModel.state.first { it.documentId != shared.documentId && it.documentId != 0L && !it.isLoading }

        viewModel.reportShareFailure(shared)
        viewModel.reportShareUnavailable(shared)

        assertEquals(replaced.status, viewModel.state.value.status)
    }

    @Test
    fun aRefusedSerializationIsReportedAndSharesNothing() = runTest {
        // A signed document: Android's save refuses to break the signature, and so does share.
        val refusal = UiText.Resource(R.string.error_core_signatures_invalidated)
        val viewModel = dispatchers.viewModel(ShareCore(PdfCoreResult.Failure(PdfCoreError.Failed(refusal))))
        viewModel.open("signed.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }

        assertNull(viewModel.shareBytes(viewModel.state.value.documentId))
        assertEquals(refusal, viewModel.state.value.status)
    }

    @Test
    fun nothingIsSharedWithoutAnOpenDocument() = runTest {
        assertNull(dispatchers.viewModel(ShareCore(PdfCoreResult.Success(byteArrayOf(1)))).shareBytes(documentId = 0L))
    }

    @Test
    fun aFailedCopyAndAMissingReceiverEachSayWhatHappened() = runTest {
        val viewModel = dispatchers.viewModel(ShareCore(PdfCoreResult.Success(byteArrayOf(1))))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        val shared = requireNotNull(viewModel.shareBytes(viewModel.state.value.documentId))

        viewModel.reportShareFailure(shared)
        assertEquals("Could not prepare the PDF to share.", viewModel.state.value.status.english())

        viewModel.reportShareUnavailable(shared)
        assertEquals("No app on this device can receive a PDF.", viewModel.state.value.status.english())
    }

    private fun ViewerViewModel.dirty() {
        setAnnotationTool(AnnotationTool.Highlight)
        placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
    }
}

private class ShareCore(private val saved: PdfCoreResult<ByteArray>) : PdfCore {
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> = PdfCoreResult.Success(ShareDocument(saved))
}

private class ShareDocument(private val saved: PdfCoreResult<ByteArray>) : PdfDocument {
    override val pageCount = 1
    override val pageSizes = listOf(PageSize(612.0, 792.0))
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun annotations(): PdfCoreResult<AnnotationSnapshot> =
        PdfCoreResult.Success(AnnotationSnapshot(emptyList(), editingAllowed = true, canUndo = false, canRedo = false))
    override fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> = PdfCoreResult.Success(Unit)
    override fun saveToBytes(): PdfCoreResult<ByteArray> = saved
    override fun close() = Unit
}
