package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelStartToolTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun eachShortcutOpensItsToolWithoutEditingTheDocument() = runTest {
        for (tool in DocumentStartTool.entries) {
            val viewModel = dispatchers.viewModel(StartToolCore())
            viewModel.open("chosen.pdf", byteArrayOf(1), saveTarget = "chosen-target", startTool = tool)
            advanceUntilIdle()
            val state = viewModel.state.value
            assertEquals(tool, state.startTool)
            assertEquals("chosen-target", state.saveTarget)
            assertFalse(state.isDirty)
            when (tool) {
                DocumentStartTool.EditText -> assertNotNull(state.contentEdit)
                DocumentStartTool.Highlight -> assertEquals(AnnotationTool.Highlight, state.activeAnnotationTool)
                DocumentStartTool.Sign -> assertNotNull(state.sign)
                DocumentStartTool.Organize -> assertNotNull(state.organize)
                DocumentStartTool.Compress -> assertNotNull(state.compress)
            }
        }
    }

    @Test
    fun encryptedShortcutWaitsForTheCorrectPassword() = runTest {
        val viewModel = dispatchers.viewModel(StartToolCore())
        viewModel.open("locked.pdf", byteArrayOf(2), startTool = DocumentStartTool.Highlight)
        advanceUntilIdle()
        assertTrue(viewModel.state.value.needsPassword)
        assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
        viewModel.retryPassword("wrong")
        advanceUntilIdle()
        assertTrue(viewModel.state.value.needsPassword)
        viewModel.retryPassword("secret")
        advanceUntilIdle()
        assertFalse(viewModel.state.value.needsPassword)
        assertEquals(AnnotationTool.Highlight, viewModel.state.value.activeAnnotationTool)
        assertEquals(ReaderMode.Edit, initialReaderMode(viewModel.state.value.startTool))
    }

    @Test
    fun cancellingThePasswordForgetsTheShortcutAndRetryBytes() = runTest {
        val viewModel = dispatchers.viewModel(StartToolCore())
        viewModel.open("locked.pdf", byteArrayOf(2), startTool = DocumentStartTool.Sign)
        advanceUntilIdle()
        viewModel.cancelPassword()
        viewModel.retryPassword("secret")
        advanceUntilIdle()
        assertEquals(0, viewModel.state.value.pageCount)
        viewModel.open("plain.pdf", byteArrayOf(1))
        advanceUntilIdle()
        assertNull(viewModel.state.value.startTool)
        assertNull(viewModel.state.value.sign)
    }

    @Test
    fun aFailedOpenDoesNotArmAToolOrLeakItToTheNextDocument() = runTest {
        val viewModel = dispatchers.viewModel(StartToolCore())
        viewModel.open("broken.pdf", byteArrayOf(3), startTool = DocumentStartTool.Organize)
        advanceUntilIdle()
        assertEquals(0, viewModel.state.value.pageCount)
        assertNull(viewModel.state.value.organize)
        viewModel.open("plain.pdf", byteArrayOf(1))
        advanceUntilIdle()
        assertNull(viewModel.state.value.organize)
        assertNull(viewModel.state.value.startTool)
    }

    @Test
    fun aSuccessfulShortcutIsNotReappliedOnALaterOpen() = runTest {
        val viewModel = dispatchers.viewModel(StartToolCore())
        viewModel.open("first.pdf", byteArrayOf(1), startTool = DocumentStartTool.Highlight)
        advanceUntilIdle()
        viewModel.open("next.pdf", byteArrayOf(1))
        advanceUntilIdle()
        assertNull(viewModel.state.value.startTool)
        assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
    }

    @Test
    fun shortcutRespectsTheDocumentsEditingPermissions() = runTest {
        for (tool in listOf(DocumentStartTool.Highlight, DocumentStartTool.EditText)) {
            val viewModel = dispatchers.viewModel(StartToolCore(editingAllowed = false))
            viewModel.open("restricted.pdf", byteArrayOf(1), startTool = tool)
            advanceUntilIdle()
            assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
            assertNull(viewModel.state.value.contentEdit)
        }
    }

    @Test
    fun readerStartsInTheTabThatOwnsTheShortcut() {
        assertEquals(ReaderMode.Read, initialReaderMode(null))
        assertEquals(ReaderMode.Edit, initialReaderMode(DocumentStartTool.EditText))
        assertEquals(ReaderMode.Edit, initialReaderMode(DocumentStartTool.Highlight))
        assertEquals(ReaderMode.Sign, initialReaderMode(DocumentStartTool.Sign))
        assertEquals(ReaderMode.Read, initialReaderMode(DocumentStartTool.Compress))
        assertEquals(ReaderMode.Read, initialReaderMode(DocumentStartTool.Organize))
    }
}

private class StartToolCore(private val editingAllowed: Boolean = true) : PdfCore {
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> = when {
        bytes.contentEquals(byteArrayOf(3)) -> PdfCoreResult.Failure(PdfCoreError.Failed("Invalid PDF"))
        bytes.contentEquals(byteArrayOf(2)) && password == null -> PdfCoreResult.Failure(PdfCoreError.PasswordRequired)
        bytes.contentEquals(byteArrayOf(2)) && password != "secret" -> PdfCoreResult.Failure(PdfCoreError.WrongPassword)
        else -> PdfCoreResult.Success(StartToolDocument(editingAllowed))
    }
}

private class StartToolDocument(private val editingAllowed: Boolean) : PdfDocument {
    override val pageCount = 1
    override val pageSizes = listOf(PageSize(612.0, 792.0))
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun annotations(): PdfCoreResult<AnnotationSnapshot> =
        PdfCoreResult.Success(AnnotationSnapshot(emptyList(), editingAllowed, canUndo = false, canRedo = false))
    override fun contentEditingAllowed() = editingAllowed
    override fun close() = Unit
}
