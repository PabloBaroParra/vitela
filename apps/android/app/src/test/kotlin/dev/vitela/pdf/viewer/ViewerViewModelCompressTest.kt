package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.CompressPreset
import dev.vitela.pdf.core.CompressedCopy
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
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Compress: a preset, then the run, then — only when something smaller came out — a destination. */
class ViewerViewModelCompressTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun openingTheDialogOffersBalanced() = runTest {
        val viewModel = openedWith(CompressDocument())
        viewModel.openCompress()

        val editor = requireNotNull(viewModel.state.first { it.compress != null }.compress)
        assertEquals(CompressPreset.Balanced, editor.preset)
        assertNull(editor.refusal)
        assertFalse(editor.signaturesWillBreak)
    }

    @Test
    fun aDocumentThatCannotBeCompressedOpensRefusedWithTheReason() = runTest {
        val document = CompressDocument(refusal = "this document cannot be rewritten")
        val viewModel = openedWith(document)
        viewModel.openCompress()

        val editor = requireNotNull(viewModel.state.first { it.compress != null }.compress)
        assertEquals("This document cannot be rewritten.", editor.refusal)
        viewModel.selectCompressPreset(CompressPreset.Small)
        assertEquals(CompressPreset.Balanced, viewModel.state.value.compress?.preset)
        assertNull(viewModel.compress())
        assertEquals(emptyList<CompressPreset>(), document.runs)
    }

    @Test
    fun aSignedDocumentWarnsBeforeTheRunAndConfirmingAcknowledgesIt() = runTest {
        val document = CompressDocument(signed = true)
        val viewModel = openedWith(document)
        viewModel.openCompress()

        assertTrue(requireNotNull(viewModel.state.first { it.compress != null }.compress).signaturesWillBreak)
        viewModel.compress()
        assertEquals(listOf(true), document.acknowledgements)
    }

    @Test
    fun anUnsignedRunIsNeverAcknowledged() = runTest {
        val document = CompressDocument()
        val viewModel = openedWith(document)
        viewModel.openCompress()
        viewModel.state.first { it.compress != null }
        viewModel.compress()

        assertEquals(listOf(false), document.acknowledgements)
    }

    @Test
    fun aFailedSignatureQueryRefusesInsteadOfGuessing() = runTest {
        val viewModel = openedWith(CompressDocument(signatureQueryFailure = "The document could not be processed."))
        viewModel.openCompress()

        val editor = requireNotNull(viewModel.state.first { it.compress != null }.compress)
        assertEquals("The document could not be processed.", editor.refusal)
    }

    @Test
    fun theChosenPresetIsTheOneThatRuns() = runTest {
        val document = CompressDocument()
        val viewModel = openedWith(document)
        viewModel.openCompress()
        viewModel.state.first { it.compress != null }
        viewModel.selectCompressPreset(CompressPreset.Small)
        viewModel.compress()

        assertEquals(listOf(CompressPreset.Small), document.runs)
    }

    @Test
    fun aReductionClosesTheDialogSuggestsANameAndWritesTheSmallerBytes() = runTest {
        val viewModel = openedWith(CompressDocument(before = 2_000_000, after = 500_000))
        viewModel.openCompress()
        viewModel.state.first { it.compress != null }

        assertEquals("a-compressed.pdf", viewModel.compress())
        val waiting = viewModel.state.value
        assertNull(waiting.compress)
        assertFalse(waiting.compressRunning)
        assertEquals("2.0 MB uncompressed → 500 kB compressed (75% smaller). Choose where to write it.", waiting.status)

        val writes = mutableListOf<List<Byte>>()
        viewModel.writeCompressed { bytes -> writes += bytes.toList(); true }

        assertEquals(listOf(listOf<Byte>(7, 7)), writes)
        val state = viewModel.state.value
        assertEquals("Compressed PDF written (2.0 MB → 500 kB, 75% smaller).", state.status)
        assertFalse(state.isDirty)
    }

    @Test
    fun noGainAsksForNoDestination() = runTest {
        val viewModel = openedWith(CompressDocument(before = 40_000, after = 40_000))
        viewModel.openCompress()
        viewModel.state.first { it.compress != null }

        assertNull(viewModel.compress())
        var writes = 0
        viewModel.writeCompressed { writes++; true }

        assertEquals(0, writes)
        assertEquals(compressNoGainSummary(CompressedCopy(ByteArray(0), 40_000, 40_000, 0, false, emptyList())), viewModel.state.value.status)
    }

    @Test
    fun aCoreFailureIsReportedAndNothingWaits() = runTest {
        val viewModel = openedWith(CompressDocument(runFailure = "Saving would invalidate this document's signature, so it was not saved."))
        viewModel.openCompress()
        viewModel.state.first { it.compress != null }

        assertNull(viewModel.compress())
        assertEquals("Saving would invalidate this document's signature, so it was not saved.", viewModel.state.value.status)
        assertFalse(viewModel.state.value.compressRunning)
    }

    @Test
    fun aFailedWriteIsReported() = runTest {
        val viewModel = openedWith(CompressDocument())
        viewModel.openCompress()
        viewModel.state.first { it.compress != null }
        viewModel.compress()
        viewModel.writeCompressed { false }

        assertEquals("Could not write the compressed PDF.", viewModel.state.value.status)
    }

    @Test
    fun cancellingThePickerDropsTheBytesAndWritesNothing() = runTest {
        val viewModel = openedWith(CompressDocument())
        viewModel.openCompress()
        viewModel.state.first { it.compress != null }
        viewModel.compress()
        viewModel.cancelCompress()
        var writes = 0
        viewModel.writeCompressed { writes++; true }

        assertEquals(0, writes)
        assertEquals("Compression cancelled. No file was written.", viewModel.state.value.status)
    }

    @Test
    fun theBytesAreSpentByTheWriteThatUsesThem() = runTest {
        val viewModel = openedWith(CompressDocument())
        viewModel.openCompress()
        viewModel.state.first { it.compress != null }
        viewModel.compress()
        var writes = 0
        viewModel.writeCompressed { writes++; true }
        viewModel.writeCompressed { writes++; true }

        assertEquals(1, writes)
    }

    @Test
    fun aDialogOpenedForOneDocumentDoesNotCompressTheNext() = runTest {
        val first = CompressDocument()
        val second = CompressDocument()
        val viewModel = dispatchers.viewModel(CompressQueueCore(first, second))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId == 1L && !it.isLoading }
        viewModel.openCompress()
        viewModel.state.first { it.compress != null }
        viewModel.open("b.pdf", byteArrayOf(2))
        viewModel.state.first { it.documentId == 2L && !it.isLoading }

        assertNull(viewModel.state.value.compress)
        assertNull(viewModel.compress())
        assertEquals(emptyList<CompressPreset>(), first.runs)
        assertEquals(emptyList<CompressPreset>(), second.runs)
    }

    @Test
    fun dismissingTheDialogRunsNothing() = runTest {
        val document = CompressDocument()
        val viewModel = openedWith(document)
        viewModel.openCompress()
        viewModel.state.first { it.compress != null }
        viewModel.dismissCompress()

        assertNull(viewModel.state.value.compress)
        assertNull(viewModel.compress())
        assertEquals(emptyList<CompressPreset>(), document.runs)
    }

    private suspend fun openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(CompressQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        return viewModel
    }
}

private class CompressQueueCore(vararg documents: PdfDocument) : PdfCore {
    private val queue = ArrayDeque(documents.toList())
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> = PdfCoreResult.Success(queue.removeFirst())
}

private class CompressDocument(
    private val refusal: String? = null,
    private val signed: Boolean = false,
    private val signatureQueryFailure: String? = null,
    private val runFailure: String? = null,
    private val before: Long = 1_000,
    private val after: Long = 400,
) : PdfDocument {
    val runs = mutableListOf<CompressPreset>()
    val acknowledgements = mutableListOf<Boolean>()
    override val pageCount = 2
    override val pageSizes = List(pageCount) { PageSize(612.0, 792.0) }
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun compressionRefusal(): String? = refusal
    override fun compressedSaveWillInvalidateSignatures(): PdfCoreResult<Boolean> =
        signatureQueryFailure?.let { PdfCoreResult.Failure(PdfCoreError.Failed(it)) } ?: PdfCoreResult.Success(signed)
    override fun saveCompressed(preset: CompressPreset, signaturesAcknowledged: Boolean): PdfCoreResult<CompressedCopy> {
        runFailure?.let { return PdfCoreResult.Failure(PdfCoreError.Failed(it)) }
        runs += preset
        acknowledgements += signaturesAcknowledged
        val reduced = after < before
        return PdfCoreResult.Success(CompressedCopy(byteArrayOf(7, 7), before, after, if (reduced) before - after else 0, reduced, emptyList()))
    }
    override fun close() = Unit
}
