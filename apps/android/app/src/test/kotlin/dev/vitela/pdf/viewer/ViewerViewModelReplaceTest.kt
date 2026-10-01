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
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test

/**
 * Replacing a document that has unsaved changes asks once. An encrypted
 * replacement fails its first open and asks for a password while the old,
 * still-unsaved document stays on screen; the reader's "discard" has to
 * survive that prompt, or the retry asks them again (#283).
 */
class ViewerViewModelReplaceTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun anEncryptedReplacementAsksToDiscardOnlyOnce() = runTest {
        val viewModel = dispatchers.viewModel(LockedCore())
        openDirty(viewModel)

        viewModel.open("locked.pdf", LOCKED)
        viewModel.state.first { it.pendingReplacementTitle != null }
        viewModel.confirmReplacement()
        viewModel.state.first { it.needsPassword }

        viewModel.retryPassword(PASSWORD)

        val settled = viewModel.settledAfterRetry()
        assertNull("the discard was already confirmed", settled.pendingReplacementTitle)
        assertFalse(settled.isDirty)
    }

    @Test
    fun aWrongPasswordKeepsTheConsentForTheNextTry() = runTest {
        val viewModel = dispatchers.viewModel(LockedCore())
        openDirty(viewModel)
        viewModel.open("locked.pdf", LOCKED)
        viewModel.state.first { it.pendingReplacementTitle != null }
        viewModel.confirmReplacement()
        viewModel.state.first { it.needsPassword }

        viewModel.retryPassword("wrong")
        val refused = viewModel.state.first { it.pendingReplacementTitle != null || it.passwordMessage != null }
        assertNull("a wrong password asks again for the password, not for the discard", refused.pendingReplacementTitle)
        viewModel.retryPassword(PASSWORD)

        assertNull(viewModel.settledAfterRetry().pendingReplacementTitle)
    }

    @Test
    fun aCancelledPasswordDoesNotCarryTheConsentToTheNextOpen() = runTest {
        val viewModel = dispatchers.viewModel(LockedCore())
        openDirty(viewModel)
        viewModel.open("locked.pdf", LOCKED)
        viewModel.state.first { it.pendingReplacementTitle != null }
        viewModel.confirmReplacement()
        viewModel.state.first { it.needsPassword }

        viewModel.cancelPassword()
        assertEquals("the unsaved document is still the open one", true, viewModel.state.value.isDirty)

        viewModel.open("other.pdf", byteArrayOf(3))
        assertEquals("other.pdf", viewModel.state.first { it.pendingReplacementTitle != null }.pendingReplacementTitle)
    }

    @Test
    fun aSpentConsentNeverDiscardsLaterEdits() = runTest {
        val viewModel = dispatchers.viewModel(LockedCore())
        openDirty(viewModel)
        viewModel.open("locked.pdf", LOCKED)
        viewModel.state.first { it.pendingReplacementTitle != null }
        viewModel.confirmReplacement()
        viewModel.state.first { it.needsPassword }
        viewModel.retryPassword(PASSWORD)
        viewModel.settledAfterRetry()

        // New unsaved work on the replacement, then a stray retry.
        viewModel.state.first { it.title == "locked.pdf" && it.annotationEditingAllowed && !it.isDirty }
        viewModel.setAnnotationTool(AnnotationTool.Highlight)
        viewModel.placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
        viewModel.state.first { it.isDirty }
        viewModel.retryPassword(PASSWORD)

        assertEquals("the consent was for the old edits, not these", "locked.pdf", viewModel.state.first { it.pendingReplacementTitle != null }.pendingReplacementTitle)
    }

    /** Either the replacement opened, or the retry stopped at a second discard prompt — the bug. */
    private suspend fun ViewerViewModel.settledAfterRetry(): ViewerState = state.first {
        it.pendingReplacementTitle != null || (it.documentId != 0L && it.title == "locked.pdf" && !it.isLoading && !it.needsPassword)
    }

    private suspend fun openDirty(viewModel: ViewerViewModel) {
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.annotationEditingAllowed }
        viewModel.setAnnotationTool(AnnotationTool.Highlight)
        viewModel.placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
        viewModel.state.first { it.isDirty }
    }

    private companion object {
        val LOCKED = byteArrayOf(2)
        const val PASSWORD = "secret"
    }
}

/** Opens anything, except [LOCKED] bytes without the right password. */
private class LockedCore : PdfCore {
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> = when {
        bytes.contentEquals(byteArrayOf(2)) && password == null -> PdfCoreResult.Failure(PdfCoreError.PasswordRequired)
        bytes.contentEquals(byteArrayOf(2)) && password != "secret" -> PdfCoreResult.Failure(PdfCoreError.WrongPassword)
        else -> PdfCoreResult.Success(ReplaceDocument())
    }
}

private class ReplaceDocument : PdfDocument {
    override val pageCount = 1
    override val pageSizes = listOf(PageSize(612.0, 792.0))
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun annotations(): PdfCoreResult<AnnotationSnapshot> =
        PdfCoreResult.Success(AnnotationSnapshot(emptyList(), editingAllowed = true, canUndo = false, canRedo = false))
    override fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> = PdfCoreResult.Success(Unit)
    override fun close() = Unit
}
