package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.ImageExportFormat
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Export images: choices, then a plan, then one file per page through the shell's writer. */
class ViewerViewModelImageExportTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun openingTheDialogOffersPngAllPagesAt150Dpi() = runTest {
        val viewModel = openedWith(PlanDocument())
        viewModel.openImageExport()

        val editor = requireNotNull(viewModel.state.first { it.imageExport != null }.imageExport)
        assertEquals(ImageExportDraft(), editor.draft)
        assertEquals(ImageExportPages.All, editor.draft.pages)
        assertEquals(ImageExportFormat.Png, editor.draft.format)
        assertEquals("150", editor.draft.dpi)
        assertTrue(editor.exportAllowed)
        assertNull(editor.message)
    }

    @Test
    fun aDocumentThatWithholdsExtractionOpensRefusedWithTheReason() = runTest {
        val viewModel = openedWith(PlanDocument(allowed = false))
        viewModel.openImageExport()

        val editor = requireNotNull(viewModel.state.first { it.imageExport != null }.imageExport)
        assertFalse(editor.exportAllowed)
        assertEquals(IMAGE_EXPORT_NOT_ALLOWED, editor.message)
        assertFalse(viewModel.planImageExport())
        assertTrue(viewModel.state.value.imageExport != null)
    }

    @Test
    fun aRefusedPlanKeepsTheDialogOpenWithTheCoresSentence() = runTest {
        val viewModel = openedWith(PlanDocument(pageCount = 9))
        viewModel.openImageExport()
        viewModel.state.first { it.imageExport != null }
        viewModel.editImageExport(ImageExportDraft(pages = ImageExportPages.Custom, customRange = "nope"))

        assertFalse(viewModel.planImageExport())
        val editor = requireNotNull(viewModel.state.value.imageExport)
        assertEquals("\"nope\" is not a page number.", editor.message)
        assertEquals("nope", editor.draft.customRange)
    }

    @Test
    fun aPlanClosesTheDialogAndExportWritesEachPageInOrder() = runTest {
        val document = PlanDocument(pageCount = 3)
        val viewModel = openedWith(document)
        viewModel.openImageExport()
        viewModel.state.first { it.imageExport != null }
        viewModel.editImageExport(ImageExportDraft(pages = ImageExportPages.Custom, customRange = "2-3", format = ImageExportFormat.Jpeg, dpi = "200"))

        assertTrue(viewModel.planImageExport())
        assertNull(viewModel.state.value.imageExport)
        val writes = mutableListOf<Triple<String, String, List<Byte>>>()
        viewModel.exportImages { name, mime, bytes -> writes += Triple(name, mime, bytes.toList()); true }

        assertEquals(
            listOf(
                Triple("a-2.jpg", "image/jpeg", listOf(1.toByte())),
                Triple("a-3.jpg", "image/jpeg", listOf(2.toByte())),
            ),
            writes,
        )
        assertEquals(listOf(Triple(1, 200, ImageExportFormat.Jpeg), Triple(2, 200, ImageExportFormat.Jpeg)), document.exported)
        val state = viewModel.state.value
        assertEquals("Exported 2 pages as JPEG.", state.status)
        assertFalse(state.imageExportRunning)
        assertFalse(state.isDirty)
    }

    @Test
    fun aSingleExportedPageIsReportedInTheSingular() = runTest {
        val viewModel = openedWith(PlanDocument(pageCount = 3))
        viewModel.openImageExport()
        viewModel.state.first { it.imageExport != null }
        viewModel.editImageExport(ImageExportDraft(pages = ImageExportPages.Current))
        viewModel.planImageExport()
        viewModel.exportImages { _, _, _ -> true }

        assertEquals("Exported 1 page as PNG.", viewModel.state.value.status)
    }

    @Test
    fun aFailedWriteStopsTheExportAndCountsWhatWasWritten() = runTest {
        val viewModel = openedWith(PlanDocument(pageCount = 3))
        viewModel.openImageExport()
        viewModel.state.first { it.imageExport != null }
        viewModel.planImageExport()
        val attempted = mutableListOf<String>()
        viewModel.exportImages { name, _, _ -> attempted += name; attempted.size < 2 }

        assertEquals(listOf("a-1.png", "a-2.png"), attempted)
        assertEquals("Could not write a-2.png. 1 written.", viewModel.state.value.status)
        assertFalse(viewModel.state.value.imageExportRunning)
    }

    @Test
    fun aPageThatCannotBeRenderedStopsTheExportBeforeWritingIt() = runTest {
        val viewModel = openedWith(PlanDocument(pageCount = 3, failingPage = 1))
        viewModel.openImageExport()
        viewModel.state.first { it.imageExport != null }
        viewModel.planImageExport()
        val written = mutableListOf<String>()
        viewModel.exportImages { name, _, _ -> written += name; true }

        assertEquals(listOf("a-1.png"), written)
        assertEquals("Page 2 could not be exported: The document could not be processed. 1 written.", viewModel.state.value.status)
    }

    @Test
    fun cancellingThePickerDropsThePlanAndWritesNothing() = runTest {
        val viewModel = openedWith(PlanDocument())
        viewModel.openImageExport()
        viewModel.state.first { it.imageExport != null }
        viewModel.planImageExport()
        viewModel.cancelImageExport()
        var writes = 0
        viewModel.exportImages { _, _, _ -> writes++; true }

        assertEquals(0, writes)
        assertEquals("Export cancelled. No file was written.", viewModel.state.value.status)
    }

    @Test
    fun aPlanIsSpentByTheExportThatUsesIt() = runTest {
        val viewModel = openedWith(PlanDocument(pageCount = 1))
        viewModel.openImageExport()
        viewModel.state.first { it.imageExport != null }
        viewModel.planImageExport()
        viewModel.exportImages { _, _, _ -> true }
        var writes = 0
        viewModel.exportImages { _, _, _ -> writes++; true }

        assertEquals(0, writes)
    }

    @Test
    fun aPlanDoesNotSurviveTheDocumentItWasMadeFor() = runTest {
        val viewModel = dispatchers.viewModel(QueueCore(PlanDocument(pageCount = 2), PlanDocument(pageCount = 2)))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId == 1L && !it.isLoading }
        viewModel.openImageExport()
        viewModel.state.first { it.imageExport != null }
        viewModel.planImageExport()
        viewModel.open("b.pdf", byteArrayOf(2))
        viewModel.state.first { it.documentId == 2L && !it.isLoading }
        var writes = 0
        viewModel.exportImages { _, _, _ -> writes++; true }

        assertEquals(0, writes)
        assertEquals("The document changed. Export cancelled.", viewModel.state.value.status)
    }

    @Test
    fun dismissingTheDialogDropsTheChoices() = runTest {
        val viewModel = openedWith(PlanDocument())
        viewModel.openImageExport()
        viewModel.state.first { it.imageExport != null }
        viewModel.editImageExport(ImageExportDraft(dpi = "300"))
        viewModel.dismissImageExport()

        assertNull(viewModel.state.value.imageExport)
    }

    private suspend fun openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(QueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        return viewModel
    }
}

private class QueueCore(vararg documents: PdfDocument) : PdfCore {
    private val queue = ArrayDeque(documents.toList())
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> = PdfCoreResult.Success(queue.removeFirst())
}
