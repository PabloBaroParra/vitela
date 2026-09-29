package dev.vitela.pdf.viewer

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

/** Protect: two passwords, then a destination, then the protected file reopened under both. */
class ViewerViewModelProtectTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun openingTheDialogOffersProtection() = runTest {
        val viewModel = openedWith(ProtectDocument())
        viewModel.openProtect()

        val editor = requireNotNull(viewModel.state.first { it.protect != null }.protect)
        assertNull(editor.refusal)
        assertNull(editor.error)
        assertFalse(editor.signaturesWillBreak)
    }

    @Test
    fun aDocumentThatForbidsProtectionChangesOpensRefused() = runTest {
        val document = ProtectDocument(allowed = false)
        val viewModel = openedWith(document)
        viewModel.openProtect()

        val editor = requireNotNull(viewModel.state.first { it.protect != null }.protect)
        assertEquals("This document does not permit protection changes.", editor.refusal)
        assertNull(viewModel.confirmProtect("user", "owner"))
        viewModel.writeProtected("a-protected.pdf", null) { true }
        assertEquals(emptyList<Pair<String, String>>(), document.protections)
    }

    @Test
    fun aFailedSignatureQueryRefusesInsteadOfGuessing() = runTest {
        val viewModel = openedWith(ProtectDocument(signatureQueryFailure = "The document could not be processed."))
        viewModel.openProtect()

        assertEquals("The document could not be processed.", requireNotNull(viewModel.state.first { it.protect != null }.protect).refusal)
    }

    @Test
    fun badPasswordsKeepTheDialogOpenWithTheReason() = runTest {
        val viewModel = openedWith(ProtectDocument())
        viewModel.openProtect()
        viewModel.state.first { it.protect != null }

        assertNull(viewModel.confirmProtect("same", "same"))
        assertEquals("The two passwords must be different.", viewModel.state.value.protect?.error)
    }

    @Test
    fun acceptedPasswordsCloseTheDialogAndSuggestAName() = runTest {
        val viewModel = openedWith(ProtectDocument())
        viewModel.openProtect()
        viewModel.state.first { it.protect != null }

        assertEquals("a-protected.pdf", viewModel.confirmProtect("user", "owner"))
        assertNull(viewModel.state.value.protect)
        assertEquals("Choose where to write the protected PDF.", viewModel.state.value.status)
    }

    @Test
    fun theProtectedFileIsWrittenAndReopenedUnderBothPasswords() = runTest {
        val document = ProtectDocument()
        val reopened = ProtectDocument()
        val core = ProtectQueueCore(document, reopened = reopened)
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        viewModel.openProtect()
        viewModel.state.first { it.protect != null }
        viewModel.confirmProtect("user", "owner")

        val writes = mutableListOf<List<Byte>>()
        viewModel.writeProtected("a-protected.pdf", "content://target") { bytes -> writes += bytes.toList(); true }

        assertEquals(listOf("user" to "owner"), document.protections)
        assertEquals(listOf(listOf<Byte>(9, 9)), writes)
        assertEquals(listOf(Triple(listOf<Byte>(9, 9), "user", "owner")), core.reopenings)
        assertTrue(document.closed)
        val state = viewModel.state.value
        assertEquals("a-protected.pdf", state.title)
        assertEquals("content://target", state.saveTarget)
        assertFalse(state.isDirty)
        assertFalse(state.protectRunning)
        assertEquals("Protected PDF saved and reopened.", state.status)
    }

    @Test
    fun aSignedDocumentWarnsBeforeTheRunAndConfirmingAcknowledgesIt() = runTest {
        val document = ProtectDocument(signed = true)
        val viewModel = openedWith(document)
        viewModel.openProtect()

        assertTrue(requireNotNull(viewModel.state.first { it.protect != null }.protect).signaturesWillBreak)
        viewModel.confirmProtect("user", "owner")
        viewModel.writeProtected("a-protected.pdf", null) { true }
        assertEquals(listOf(true), document.acknowledgements)
    }

    @Test
    fun anUnsignedRunIsNeverAcknowledged() = runTest {
        val document = ProtectDocument()
        val viewModel = openedWith(document)
        viewModel.openProtect()
        viewModel.state.first { it.protect != null }
        viewModel.confirmProtect("user", "owner")
        viewModel.writeProtected("a-protected.pdf", null) { true }

        assertEquals(listOf(false), document.acknowledgements)
    }

    @Test
    fun aCoreFailureIsReportedAndNothingIsWritten() = runTest {
        val viewModel = openedWith(ProtectDocument(protectFailure = "Saving would invalidate this document's signature, so it was not saved."))
        viewModel.openProtect()
        viewModel.state.first { it.protect != null }
        viewModel.confirmProtect("user", "owner")
        var writes = 0
        viewModel.writeProtected("a-protected.pdf", null) { writes++; true }

        assertEquals(0, writes)
        assertEquals("Saving would invalidate this document's signature, so it was not saved.", viewModel.state.value.status)
        assertFalse(viewModel.state.value.protectRunning)
    }

    @Test
    fun aFailedWriteIsReportedAndTheDocumentStaysOpen() = runTest {
        val document = ProtectDocument()
        val core = ProtectQueueCore(document, reopened = ProtectDocument())
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        viewModel.openProtect()
        viewModel.state.first { it.protect != null }
        viewModel.confirmProtect("user", "owner")
        viewModel.writeProtected("a-protected.pdf", null) { false }

        assertEquals("Could not write the protected PDF.", viewModel.state.value.status)
        assertEquals("a.pdf", viewModel.state.value.title)
        assertEquals(emptyList<Triple<List<Byte>, String, String>>(), core.reopenings)
        assertFalse(document.closed)
    }

    @Test
    fun aFailedReopenSaysTheFileWasStillWritten() = runTest {
        val core = ProtectQueueCore(ProtectDocument(), reopenFailure = PdfCoreError.WrongPassword)
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        viewModel.openProtect()
        viewModel.state.first { it.protect != null }
        viewModel.confirmProtect("user", "owner")
        viewModel.writeProtected("a-protected.pdf", null) { true }

        assertEquals("The protected PDF was written, but it could not be reopened. This document requires a password.", viewModel.state.value.status)
        assertEquals("a.pdf", viewModel.state.value.title)
    }

    @Test
    fun cancellingThePickerWritesNothing() = runTest {
        val document = ProtectDocument()
        val viewModel = openedWith(document)
        viewModel.openProtect()
        viewModel.state.first { it.protect != null }
        viewModel.confirmProtect("user", "owner")
        viewModel.cancelProtect()
        var writes = 0
        viewModel.writeProtected("a-protected.pdf", null) { writes++; true }

        assertEquals(0, writes)
        assertEquals(emptyList<Pair<String, String>>(), document.protections)
        assertEquals("Protection cancelled. No file was written.", viewModel.state.value.status)
    }

    @Test
    fun aRequestMadeForOneDocumentDoesNotProtectTheNext() = runTest {
        val first = ProtectDocument()
        val second = ProtectDocument()
        val viewModel = dispatchers.viewModel(ProtectQueueCore(first, second, reopened = ProtectDocument()))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId == 1L && !it.isLoading }
        viewModel.openProtect()
        viewModel.state.first { it.protect != null }
        viewModel.confirmProtect("user", "owner")
        viewModel.open("b.pdf", byteArrayOf(2))
        viewModel.state.first { it.documentId == 2L && !it.isLoading }
        var writes = 0
        viewModel.writeProtected("a-protected.pdf", null) { writes++; true }

        assertEquals(0, writes)
        assertEquals(emptyList<Pair<String, String>>(), first.protections)
        assertEquals(emptyList<Pair<String, String>>(), second.protections)
        assertEquals("b.pdf", viewModel.state.value.title)
    }

    @Test
    fun dismissingTheDialogProtectsNothing() = runTest {
        val document = ProtectDocument()
        val viewModel = openedWith(document)
        viewModel.openProtect()
        viewModel.state.first { it.protect != null }
        viewModel.dismissProtect()

        assertNull(viewModel.state.value.protect)
        assertNull(viewModel.confirmProtect("user", "owner"))
    }

    private suspend fun openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(ProtectQueueCore(document, reopened = ProtectDocument()))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        return viewModel
    }
}

private class ProtectQueueCore(
    vararg documents: PdfDocument,
    private val reopened: PdfDocument? = null,
    private val reopenFailure: PdfCoreError? = null,
) : PdfCore {
    private val queue = ArrayDeque(documents.toList())
    val reopenings = mutableListOf<Triple<List<Byte>, String, String>>()
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> = PdfCoreResult.Success(queue.removeFirst())
    override fun openWithPasswords(bytes: ByteArray, openPassword: String, permissionsPassword: String): PdfCoreResult<PdfDocument> {
        reopenFailure?.let { return PdfCoreResult.Failure(it) }
        reopenings += Triple(bytes.toList(), openPassword, permissionsPassword)
        return PdfCoreResult.Success(requireNotNull(reopened))
    }
}

private class ProtectDocument(
    private val allowed: Boolean = true,
    private val signed: Boolean = false,
    private val signatureQueryFailure: String? = null,
    private val protectFailure: String? = null,
) : PdfDocument {
    val protections = mutableListOf<Pair<String, String>>()
    val acknowledgements = mutableListOf<Boolean>()
    var closed = false
    override val pageCount = 2
    override val pageSizes = List(pageCount) { PageSize(612.0, 792.0) }
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun protectionChangeAllowed(): Boolean = allowed
    override fun protectionWillInvalidateSignatures(): PdfCoreResult<Boolean> =
        signatureQueryFailure?.let { PdfCoreResult.Failure(PdfCoreError.Failed(it)) } ?: PdfCoreResult.Success(signed)
    override fun protect(openPassword: String, permissionsPassword: String, signaturesAcknowledged: Boolean): PdfCoreResult<ByteArray> {
        protectFailure?.let { return PdfCoreResult.Failure(PdfCoreError.Failed(it)) }
        protections += openPassword to permissionsPassword
        acknowledgements += signaturesAcknowledged
        return PdfCoreResult.Success(byteArrayOf(9, 9))
    }
    override fun close() {
        closed = true
    }
}
