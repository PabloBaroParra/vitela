package dev.vitela.pdf.recovery

import dev.vitela.pdf.core.CompressPreset
import dev.vitela.pdf.core.CoreFailure
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RecoverySnapshot
import dev.vitela.pdf.core.SigningCertificate
import dev.vitela.pdf.core.SigningIdentity
import dev.vitela.pdf.core.toPdfCoreError
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SignatureBrokenDocumentTest {
    private val calls = mutableListOf<String>()
    private val plain = RecoverableDocument()
    private val document = object : PdfDocument by plain {
        override fun saveToBytes() = plain.saveToBytes().also { calls += "save" }
        override fun sign(certificate: SigningCertificate, identityId: String) = plain.sign(certificate, identityId).also { calls += "sign" }
        override fun saveCompressed(preset: CompressPreset, signaturesAcknowledged: Boolean) =
            plain.saveCompressed(preset, signaturesAcknowledged).also { calls += "compress $preset $signaturesAcknowledged" }
        override fun protect(openPassword: String, permissionsPassword: String, signaturesAcknowledged: Boolean) =
            plain.protect(openPassword, permissionsPassword, signaturesAcknowledged).also { calls += "protect $openPassword $permissionsPassword $signaturesAcknowledged" }
    }
    private val certificate = object : SigningCertificate {
        override val identities = emptyList<SigningIdentity>()
        override fun close() = Unit
    }
    private val broken = SignatureBrokenDocument(document)
    private val refused = PdfCoreResult.Failure(CoreFailure.SignaturesWouldBeInvalidated.toPdfCoreError(""))
    private val preset = CompressPreset.entries.first()

    @Test
    fun anOrdinarySaveAndASignatureAreRefusedWithoutReachingTheDocument() {
        assertEquals(refused, broken.saveToBytes())
        assertEquals(refused, broken.sign(certificate, "id"))
        assertEquals(emptyList<String>(), calls)
    }

    @Test
    fun compressAndProtectWarnAndNeedTheYes() {
        assertEquals(PdfCoreResult.Success(true), broken.compressedSaveWillInvalidateSignatures())
        assertEquals(PdfCoreResult.Success(true), broken.protectionWillInvalidateSignatures())
        assertEquals(refused, broken.saveCompressed(preset, signaturesAcknowledged = false))
        assertEquals(refused, broken.protect("a", "b", signaturesAcknowledged = false))
        assertEquals(emptyList<String>(), calls)

        broken.saveCompressed(preset, true)
        broken.protect("a", "b", true)
        assertEquals(listOf("compress $preset true", "protect a b true"), calls)
    }

    @Test
    fun itsOwnRecoveryCopyKeepsTheFlag() {
        val snapshot = broken.recoverySnapshot() as PdfCoreResult.Success<RecoverySnapshot>
        assertTrue(snapshot.value.breaksSignatures)
    }

    @Test
    fun everythingElseIsTheDocumentsOwn() {
        assertEquals(document.pageCount, broken.pageCount)
        plain.encrypted = true
        assertTrue(broken.holdsEncryptedContent())
    }
}
