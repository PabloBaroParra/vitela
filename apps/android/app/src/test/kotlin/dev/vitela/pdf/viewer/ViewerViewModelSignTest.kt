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
import dev.vitela.pdf.core.SigningCertificate
import dev.vitela.pdf.core.SigningIdentity
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Sign: a certificate file, its password, who to sign as; then a destination, and the signed file reopened. */
class ViewerViewModelSignTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun openingTheDialogAsksForACertificate() = runTest {
        val viewModel = openedWith(SignDocument())
        viewModel.openSign()

        assertEquals(SignEditor(), viewModel.state.first { it.sign != null }.sign)
    }

    @Test
    fun aRefusingDocumentSaysWhyInTheCoresWords() = runTest {
        val viewModel = openedWith(SignDocument(refusal = "this document does not permit adding a signature"))
        viewModel.openSign()

        assertEquals("This document does not permit adding a signature.", viewModel.state.first { it.sign != null }.sign?.refusal)
        viewModel.chooseSigningCertificate("id.pfx", byteArrayOf(1))
        assertNull(viewModel.state.value.sign?.certificateName)
    }

    @Test
    fun unsavedChangesRefuseSigning() = runTest {
        val viewModel = openedWith(SignDocument())
        viewModel.state.first { it.annotationEditingAllowed }
        viewModel.setAnnotationTool(AnnotationTool.Highlight)
        viewModel.placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
        viewModel.state.first { it.isDirty }
        viewModel.openSign()

        assertEquals(SAVE_BEFORE_SIGNING, viewModel.state.first { it.sign != null }.sign?.refusal)
    }

    @Test
    fun theRightPasswordListsWhoTheFileCanSignAs() = runTest {
        val core = SignCore(SignDocument())
        val viewModel = openedWith(core)
        viewModel.openSign()
        viewModel.state.first { it.sign != null }
        viewModel.chooseSigningCertificate("id.pfx", byteArrayOf(7))
        assertEquals("id.pfx", viewModel.state.value.sign?.certificateName)

        viewModel.unlockSigningCertificate("secret")

        val editor = requireNotNull(viewModel.state.first { it.sign?.identities?.isNotEmpty() == true }.sign)
        assertEquals(listOf(SigningIdentity("alice", "Alice"), SigningIdentity("bob", "Bob")), editor.identities)
        assertEquals("alice", editor.selectedIdentityId)
        assertEquals(listOf(listOf<Byte>(7) to "secret"), core.unlocks)
    }

    @Test
    fun aWrongPasswordKeepsTheDialogOnThePassword() = runTest {
        val viewModel = openedWith(SignCore(SignDocument()))
        viewModel.openSign()
        viewModel.state.first { it.sign != null }
        viewModel.chooseSigningCertificate("id.pfx", byteArrayOf(7))
        viewModel.unlockSigningCertificate("wrong")

        val editor = requireNotNull(viewModel.state.first { it.sign?.error != null }.sign)
        assertEquals(CERTIFICATE_LOCKED, editor.error)
        assertTrue(editor.identities.isEmpty())
        assertFalse(editor.unlocking)
    }

    @Test
    fun aFileWithNoSigningIdentityIsRefusedAndClosed() = runTest {
        val certificate = FakeCertificate(emptyList())
        val viewModel = openedWith(SignCore(SignDocument(), certificate = certificate))
        viewModel.openSign()
        viewModel.state.first { it.sign != null }
        viewModel.chooseSigningCertificate("certs-only.p12", byteArrayOf(7))
        viewModel.unlockSigningCertificate("secret")

        assertEquals(CERTIFICATE_CANNOT_SIGN, viewModel.state.first { it.sign?.error != null }.sign?.error)
        assertTrue(certificate.closed)
    }

    @Test
    fun theSignedFileIsWrittenReopenedAndTheKeysClosed() = runTest {
        val document = SignDocument()
        val certificate = FakeCertificate()
        val reopened = SignDocument()
        val core = SignCore(document, reopened, certificate = certificate)
        val viewModel = unlocked(core)
        viewModel.selectSigningIdentity("bob")

        assertEquals("a-signed.pdf", viewModel.confirmSign())
        assertNull(viewModel.state.value.sign)
        assertEquals("Choose where to write the signed PDF.", viewModel.state.value.status)
        val writes = mutableListOf<List<Byte>>()
        viewModel.writeSigned("a-signed.pdf", "content://signed") { bytes -> writes += bytes.toList(); true }

        assertEquals(listOf("bob"), document.signatures)
        assertEquals(listOf(listOf<Byte>(5, 5)), writes)
        assertEquals(listOf(listOf<Byte>(5, 5)), core.reopenings)
        assertTrue(certificate.closed)
        assertTrue(document.closed)
        val state = viewModel.state.value
        assertEquals("a-signed.pdf", state.title)
        assertEquals("content://signed", state.saveTarget)
        assertFalse(state.signRunning)
        assertEquals("Signed PDF saved and reopened.", state.status)
    }

    @Test
    fun anEncryptedSignedFileAsksForItsPasswordToReopen() = runTest {
        val core = SignCore(SignDocument(), reopenFailure = PdfCoreError.PasswordRequired)
        val viewModel = unlocked(core)
        viewModel.confirmSign()
        viewModel.writeSigned("a-signed.pdf", null) { true }

        val state = viewModel.state.value
        assertTrue(state.needsPassword)
        assertEquals("a-signed.pdf", state.title)
        assertEquals("Signed PDF saved. Enter its password to reopen it.", state.status)
    }

    @Test
    fun aCoreFailureIsReportedNothingIsWrittenAndTheKeysClosed() = runTest {
        val certificate = FakeCertificate()
        val viewModel = unlocked(SignCore(SignDocument(signFailure = "That identity is not in the certificate file."), certificate = certificate))
        viewModel.confirmSign()
        var writes = 0
        viewModel.writeSigned("a-signed.pdf", null) { writes++; true }

        assertEquals(0, writes)
        assertEquals("That identity is not in the certificate file.", viewModel.state.value.status)
        assertTrue(certificate.closed)
    }

    @Test
    fun cancellingThePickerSignsNothingAndClosesTheKeys() = runTest {
        val document = SignDocument()
        val certificate = FakeCertificate()
        val viewModel = unlocked(SignCore(document, certificate = certificate))
        viewModel.confirmSign()
        viewModel.cancelSign()
        viewModel.writeSigned("a-signed.pdf", null) { true }

        assertEquals(emptyList<String>(), document.signatures)
        assertTrue(certificate.closed)
        assertEquals("Signing cancelled. No file was written.", viewModel.state.value.status)
    }

    @Test
    fun dismissingTheDialogClosesTheKeys() = runTest {
        val certificate = FakeCertificate()
        val viewModel = unlocked(SignCore(SignDocument(), certificate = certificate))
        viewModel.dismissSign()

        assertNull(viewModel.state.value.sign)
        assertTrue(certificate.closed)
        assertNull(viewModel.confirmSign())
    }

    @Test
    fun aRequestMadeForOneDocumentDoesNotSignTheNext() = runTest {
        val first = SignDocument()
        val second = SignDocument()
        val certificate = FakeCertificate()
        val viewModel = unlocked(SignCore(first, second, certificate = certificate))
        viewModel.confirmSign()
        viewModel.open("b.pdf", byteArrayOf(2))
        viewModel.state.first { it.documentId == 2L && !it.isLoading }
        var writes = 0
        viewModel.writeSigned("a-signed.pdf", null) { writes++; true }

        assertEquals(0, writes)
        assertEquals(emptyList<String>(), first.signatures)
        assertTrue(certificate.closed)
    }

    private suspend fun unlocked(core: SignCore): ViewerViewModel {
        val viewModel = openedWith(core)
        viewModel.openSign()
        viewModel.state.first { it.sign != null }
        viewModel.chooseSigningCertificate("id.pfx", byteArrayOf(7))
        viewModel.unlockSigningCertificate("secret")
        viewModel.state.first { it.sign?.identities?.isNotEmpty() == true }
        return viewModel
    }

    private suspend fun openedWith(document: SignDocument): ViewerViewModel = openedWith(SignCore(document))

    private suspend fun openedWith(core: SignCore): ViewerViewModel {
        val viewModel = dispatchers.viewModel(core)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        return viewModel
    }
}

private class SignCore(
    vararg documents: PdfDocument,
    private val certificate: FakeCertificate = FakeCertificate(),
    private val reopenFailure: PdfCoreError? = null,
) : PdfCore {
    private val queue = ArrayDeque(documents.toList())
    val unlocks = mutableListOf<Pair<List<Byte>, String>>()
    val reopenings = mutableListOf<List<Byte>>()

    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> {
        if (bytes.contentEquals(byteArrayOf(5, 5))) {
            reopenFailure?.let { return PdfCoreResult.Failure(it) }
            reopenings += bytes.toList()
            return PdfCoreResult.Success(queue.removeFirstOrNull() ?: SignDocument())
        }
        return PdfCoreResult.Success(queue.removeFirst())
    }

    override fun openSigningCertificate(bytes: ByteArray, password: String): PdfCoreResult<SigningCertificate> {
        if (password != "secret") return PdfCoreResult.Failure(PdfCoreError.WrongPassword)
        unlocks += bytes.toList() to password
        return PdfCoreResult.Success(certificate)
    }
}

private class FakeCertificate(
    override val identities: List<SigningIdentity> = listOf(SigningIdentity("alice", "Alice"), SigningIdentity("bob", "Bob")),
) : SigningCertificate {
    var closed = false
    override fun close() {
        closed = true
    }
}

private class SignDocument(
    private val refusal: String? = null,
    private val signFailure: String? = null,
) : PdfDocument {
    val signatures = mutableListOf<String>()
    var closed = false
    override val pageCount = 2
    override val pageSizes = List(pageCount) { PageSize(612.0, 792.0) }
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun annotations(): PdfCoreResult<AnnotationSnapshot> = PdfCoreResult.Success(AnnotationSnapshot(emptyList(), editingAllowed = true, canUndo = false, canRedo = false))
    override fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> = PdfCoreResult.Success(Unit)
    override fun signingRefusal(): String? = refusal
    override fun sign(certificate: SigningCertificate, identityId: String): PdfCoreResult<ByteArray> {
        signFailure?.let { return PdfCoreResult.Failure(PdfCoreError.Failed(it)) }
        signatures += identityId
        return PdfCoreResult.Success(byteArrayOf(5, 5))
    }
    override fun close() {
        closed = true
    }
}
