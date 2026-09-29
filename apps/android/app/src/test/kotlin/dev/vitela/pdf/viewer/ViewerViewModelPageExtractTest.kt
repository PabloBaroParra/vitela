package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** Extract pages: a range, then a plan, then one new PDF through the shell's writer. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelPageExtractTest {
    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun openingTheDialogStartsWithAnEmptyRange() = runTest {
        val viewModel = openedWith(ExtractDocument())
        viewModel.openPageExtract()

        val editor = requireNotNull(viewModel.state.first { it.pageExtract != null }.pageExtract)
        assertEquals("", editor.range)
        assertTrue(editor.extractAllowed)
        assertNull(editor.message)
    }

    @Test
    fun aDocumentThatWithholdsExtractionOpensRefusedWithTheReason() = runTest {
        val viewModel = openedWith(ExtractDocument(extractionAllowed = false))
        viewModel.openPageExtract()

        val editor = requireNotNull(viewModel.state.first { it.pageExtract != null }.pageExtract)
        assertFalse(editor.extractAllowed)
        assertEquals(PAGE_EXTRACT_NOT_ALLOWED, editor.message)
        viewModel.editPageExtract("1-2")
        assertEquals("", viewModel.state.value.pageExtract?.range)
        assertNull(viewModel.planPageExtract())
    }

    @Test
    fun aDocumentThatCannotBeRewrittenOpensRefusedWithTheReason() = runTest {
        val viewModel = openedWith(ExtractDocument(rewriteAllowed = false))
        viewModel.openPageExtract()

        val editor = requireNotNull(viewModel.state.first { it.pageExtract != null }.pageExtract)
        assertFalse(editor.extractAllowed)
        assertEquals(PAGE_EXTRACT_NO_REWRITE, editor.message)
    }

    @Test
    fun aRefusedRangeKeepsTheDialogOpenWithTheCoresSentence() = runTest {
        val viewModel = openedWith(ExtractDocument(pageCount = 9))
        viewModel.openPageExtract()
        viewModel.state.first { it.pageExtract != null }
        viewModel.editPageExtract("nope")

        assertNull(viewModel.planPageExtract())
        val editor = requireNotNull(viewModel.state.value.pageExtract)
        assertEquals("\"nope\" is not a page number.", editor.message)
        assertEquals("nope", editor.range)
    }

    @Test
    fun editingTheRangeClearsTheLastRefusal() = runTest {
        val viewModel = openedWith(ExtractDocument())
        viewModel.openPageExtract()
        viewModel.state.first { it.pageExtract != null }
        viewModel.planPageExtract()
        viewModel.editPageExtract("1")

        assertNull(viewModel.state.value.pageExtract?.message)
    }

    @Test
    fun aPlanClosesTheDialogSuggestsANameAndWritesTheExtractedPdf() = runTest {
        val document = ExtractDocument(pageCount = 5)
        val viewModel = openedWith(document)
        viewModel.openPageExtract()
        viewModel.state.first { it.pageExtract != null }
        viewModel.editPageExtract("2-3")

        assertEquals("a-extract.pdf", viewModel.planPageExtract())
        assertNull(viewModel.state.value.pageExtract)
        val writes = mutableListOf<List<Byte>>()
        viewModel.extractPages { bytes -> writes += bytes.toList(); true }

        assertEquals(listOf(listOf<Byte>(1, 2)), writes)
        assertEquals(listOf(listOf(1, 2)), document.extracted)
        val state = viewModel.state.value
        assertEquals("Extracted 2 pages to a new PDF.", state.status)
        assertFalse(state.isDirty)
    }

    @Test
    fun aSignedSourceIsMentionedInTheSummary() = runTest {
        val viewModel = openedWith(ExtractDocument(signed = true))
        viewModel.openPageExtract()
        viewModel.state.first { it.pageExtract != null }
        viewModel.editPageExtract("1-1")
        viewModel.planPageExtract()
        viewModel.extractPages { true }

        assertEquals(pageExtractSummary(1, sourceIsSigned = true), viewModel.state.value.status)
    }

    @Test
    fun aCoreRefusalAtExtractTimeIsReportedAndNothingIsWritten() = runTest {
        val viewModel = openedWith(ExtractDocument(failure = "The document could not be processed."))
        viewModel.openPageExtract()
        viewModel.state.first { it.pageExtract != null }
        viewModel.editPageExtract("1-2")
        viewModel.planPageExtract()
        var writes = 0
        viewModel.extractPages { writes++; true }

        assertEquals(0, writes)
        assertEquals("The document could not be processed.", viewModel.state.value.status)
    }

    @Test
    fun aFailedWriteIsReported() = runTest {
        val viewModel = openedWith(ExtractDocument())
        viewModel.openPageExtract()
        viewModel.state.first { it.pageExtract != null }
        viewModel.editPageExtract("1-2")
        viewModel.planPageExtract()
        viewModel.extractPages { false }

        assertEquals("Could not write the extracted PDF.", viewModel.state.value.status)
    }

    @Test
    fun cancellingThePickerDropsThePlanAndWritesNothing() = runTest {
        val viewModel = openedWith(ExtractDocument())
        viewModel.openPageExtract()
        viewModel.state.first { it.pageExtract != null }
        viewModel.editPageExtract("1-2")
        viewModel.planPageExtract()
        viewModel.cancelPageExtract()
        var writes = 0
        viewModel.extractPages { writes++; true }

        assertEquals(0, writes)
        assertEquals("Extract cancelled. No file was written.", viewModel.state.value.status)
    }

    @Test
    fun aPlanIsSpentByTheExtractThatUsesIt() = runTest {
        val document = ExtractDocument()
        val viewModel = openedWith(document)
        viewModel.openPageExtract()
        viewModel.state.first { it.pageExtract != null }
        viewModel.editPageExtract("1-1")
        viewModel.planPageExtract()
        viewModel.extractPages { true }
        viewModel.extractPages { true }

        assertEquals(1, document.extracted.size)
    }

    @Test
    fun aPlanDoesNotSurviveTheDocumentItWasMadeFor() = runTest {
        val first = ExtractDocument()
        val viewModel = ViewerViewModel(ExtractQueueCore(first, ExtractDocument()))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId == 1L && !it.isLoading }
        viewModel.openPageExtract()
        viewModel.state.first { it.pageExtract != null }
        viewModel.editPageExtract("1-1")
        viewModel.planPageExtract()
        viewModel.open("b.pdf", byteArrayOf(2))
        viewModel.state.first { it.documentId == 2L && !it.isLoading }
        var writes = 0
        viewModel.extractPages { writes++; true }

        assertEquals(0, writes)
        assertEquals(emptyList<List<Int>>(), first.extracted)
        assertEquals("The document changed. Extract cancelled.", viewModel.state.value.status)
    }

    @Test
    fun dismissingTheDialogDropsTheRange() = runTest {
        val viewModel = openedWith(ExtractDocument())
        viewModel.openPageExtract()
        viewModel.state.first { it.pageExtract != null }
        viewModel.editPageExtract("1-2")
        viewModel.dismissPageExtract()

        assertNull(viewModel.state.value.pageExtract)
    }

    private suspend fun openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = ViewerViewModel(ExtractQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        return viewModel
    }
}

private class ExtractQueueCore(vararg documents: PdfDocument) : PdfCore {
    private val queue = ArrayDeque(documents.toList())
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> = PdfCoreResult.Success(queue.removeFirst())
}
