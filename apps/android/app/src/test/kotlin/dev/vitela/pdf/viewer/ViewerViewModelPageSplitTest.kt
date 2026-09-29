package dev.vitela.pdf.viewer

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

/** Split: cuts, then a plan, then one new PDF per part through the shell's writer. */
class ViewerViewModelPageSplitTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun openingTheDialogStartsWithNoCuts() = runTest {
        val viewModel = openedWith(SplitDocument())
        viewModel.openPageSplit()

        val editor = requireNotNull(viewModel.state.first { it.pageSplit != null }.pageSplit)
        assertEquals("", editor.cuts)
        assertTrue(editor.splitAllowed)
        assertNull(editor.message)
    }

    @Test
    fun aOnePageDocumentDoesNotOpenTheDialog() = runTest {
        val viewModel = openedWith(SplitDocument(pageCount = 1))
        viewModel.openPageSplit()

        assertNull(viewModel.state.value.pageSplit)
    }

    @Test
    fun aDocumentThatWithholdsExtractionOpensRefusedWithTheReason() = runTest {
        val viewModel = openedWith(SplitDocument(extractionAllowed = false))
        viewModel.openPageSplit()

        val editor = requireNotNull(viewModel.state.first { it.pageSplit != null }.pageSplit)
        assertFalse(editor.splitAllowed)
        assertEquals(PAGE_EXTRACT_NOT_ALLOWED, editor.message)
        viewModel.editPageSplit("1")
        assertEquals("", viewModel.state.value.pageSplit?.cuts)
        assertFalse(viewModel.planPageSplit())
    }

    @Test
    fun aDocumentThatCannotBeRewrittenOpensRefusedWithTheReason() = runTest {
        val viewModel = openedWith(SplitDocument(rewriteAllowed = false))
        viewModel.openPageSplit()

        val editor = requireNotNull(viewModel.state.first { it.pageSplit != null }.pageSplit)
        assertFalse(editor.splitAllowed)
        assertEquals(PAGE_SPLIT_NO_REWRITE, editor.message)
    }

    @Test
    fun refusedCutsKeepTheDialogOpenWithTheCoresSentence() = runTest {
        val viewModel = openedWith(SplitDocument())
        viewModel.openPageSplit()
        viewModel.state.first { it.pageSplit != null }
        viewModel.editPageSplit("nope")

        assertFalse(viewModel.planPageSplit())
        val editor = requireNotNull(viewModel.state.value.pageSplit)
        assertEquals("\"nope\" is not a page number.", editor.message)
        assertEquals("nope", editor.cuts)
    }

    @Test
    fun editingTheCutsClearsTheLastRefusal() = runTest {
        val viewModel = openedWith(SplitDocument())
        viewModel.openPageSplit()
        viewModel.state.first { it.pageSplit != null }
        viewModel.planPageSplit()
        viewModel.editPageSplit("1")

        assertNull(viewModel.state.value.pageSplit?.message)
    }

    @Test
    fun aPlanClosesTheDialogAndWritesEveryPartInOrder() = runTest {
        val document = SplitDocument(pageCount = 5)
        val viewModel = openedWith(document)
        viewModel.openPageSplit()
        viewModel.state.first { it.pageSplit != null }
        viewModel.editPageSplit("2,3")

        assertTrue(viewModel.planPageSplit())
        assertNull(viewModel.state.value.pageSplit)
        val writes = mutableListOf<Pair<String, List<Byte>>>()
        viewModel.splitPages { name, bytes -> writes += name to bytes.toList(); true }

        assertEquals(
            listOf("a.pdf-part1.pdf" to listOf<Byte>(0, 1), "a.pdf-part2.pdf" to listOf<Byte>(2), "a.pdf-part3.pdf" to listOf<Byte>(3, 4)),
            writes,
        )
        val state = viewModel.state.value
        assertEquals("Split into 3 PDFs.", state.status)
        assertFalse(state.pageSplitRunning)
        assertFalse(state.isDirty)
    }

    @Test
    fun aSignedSourceIsMentionedInTheSummary() = runTest {
        val viewModel = openedWith(SplitDocument(signed = true))
        viewModel.openPageSplit()
        viewModel.state.first { it.pageSplit != null }
        viewModel.editPageSplit("1")
        viewModel.planPageSplit()
        viewModel.splitPages { _, _ -> true }

        assertEquals(pageSplitSummary(2, sourceIsSigned = true), viewModel.state.value.status)
    }

    @Test
    fun aCoreRefusalStopsTheSplitAndCountsWhatWasWritten() = runTest {
        val viewModel = openedWith(SplitDocument(failAtPart = 2))
        viewModel.openPageSplit()
        viewModel.state.first { it.pageSplit != null }
        viewModel.editPageSplit("1,2")
        viewModel.planPageSplit()
        val names = mutableListOf<String>()
        viewModel.splitPages { name, _ -> names += name; true }

        assertEquals(listOf("a.pdf-part1.pdf"), names)
        assertEquals("Split stopped after 1 written: The document could not be processed.", viewModel.state.value.status)
        assertFalse(viewModel.state.value.pageSplitRunning)
    }

    @Test
    fun aFailedWriteStopsTheSplit() = runTest {
        val document = SplitDocument()
        val viewModel = openedWith(document)
        viewModel.openPageSplit()
        viewModel.state.first { it.pageSplit != null }
        viewModel.editPageSplit("1,2")
        viewModel.planPageSplit()
        viewModel.splitPages { _, _ -> false }

        assertEquals(1, document.extracted.size)
        assertEquals("Split stopped after 0 written: could not write a.pdf-part1.pdf.", viewModel.state.value.status)
    }

    @Test
    fun cancellingThePickerDropsThePlanAndWritesNothing() = runTest {
        val viewModel = openedWith(SplitDocument())
        viewModel.openPageSplit()
        viewModel.state.first { it.pageSplit != null }
        viewModel.editPageSplit("1")
        viewModel.planPageSplit()
        viewModel.cancelPageSplit()
        var writes = 0
        viewModel.splitPages { _, _ -> writes++; true }

        assertEquals(0, writes)
        assertEquals("Split cancelled. No file was written.", viewModel.state.value.status)
    }

    @Test
    fun aPlanIsSpentByTheSplitThatUsesIt() = runTest {
        val document = SplitDocument()
        val viewModel = openedWith(document)
        viewModel.openPageSplit()
        viewModel.state.first { it.pageSplit != null }
        viewModel.editPageSplit("1")
        viewModel.planPageSplit()
        viewModel.splitPages { _, _ -> true }
        viewModel.splitPages { _, _ -> true }

        assertEquals(2, document.extracted.size)
    }

    @Test
    fun aPlanDoesNotSurviveTheDocumentItWasMadeFor() = runTest {
        val first = SplitDocument()
        val viewModel = dispatchers.viewModel(SplitQueueCore(first, SplitDocument()))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId == 1L && !it.isLoading }
        viewModel.openPageSplit()
        viewModel.state.first { it.pageSplit != null }
        viewModel.editPageSplit("1")
        viewModel.planPageSplit()
        viewModel.open("b.pdf", byteArrayOf(2))
        viewModel.state.first { it.documentId == 2L && !it.isLoading }
        var writes = 0
        viewModel.splitPages { _, _ -> writes++; true }

        assertEquals(0, writes)
        assertEquals(emptyList<List<Int>>(), first.extracted)
        assertEquals("The document changed. Split stopped after 0 written.", viewModel.state.value.status)
    }

    @Test
    fun dismissingTheDialogDropsTheCuts() = runTest {
        val viewModel = openedWith(SplitDocument())
        viewModel.openPageSplit()
        viewModel.state.first { it.pageSplit != null }
        viewModel.editPageSplit("1")
        viewModel.dismissPageSplit()

        assertNull(viewModel.state.value.pageSplit)
    }

    private suspend fun openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(SplitQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        return viewModel
    }
}

private class SplitQueueCore(vararg documents: PdfDocument) : PdfCore {
    private val queue = ArrayDeque(documents.toList())
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> = PdfCoreResult.Success(queue.removeFirst())
}
