package dev.vitela.pdf.recovery

import dev.vitela.pdf.core.CompressPreset
import dev.vitela.pdf.core.CompressedCopy
import dev.vitela.pdf.core.CoreFailure
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RecoverySnapshot
import dev.vitela.pdf.core.SigningCertificate
import dev.vitela.pdf.core.toPdfCoreError

/**
 * A restored document whose edits broke a signature before the crash. Those
 * edits are in its base now, where the core no longer sees them breaking
 * anything, so this answers the signature questions as the crashed session
 * did: an ordinary save is refused, and Compress and Protect warn first.
 * Everything else is [document]'s own.
 */
internal class SignatureBrokenDocument(private val document: PdfDocument) : PdfDocument by document {
    private val refused = PdfCoreResult.Failure(CoreFailure.SignaturesWouldBeInvalidated.toPdfCoreError(""))

    override fun saveToBytes(): PdfCoreResult<ByteArray> = refused

    override fun sign(certificate: SigningCertificate, identityId: String): PdfCoreResult<ByteArray> = refused

    override fun compressedSaveWillInvalidateSignatures(): PdfCoreResult<Boolean> = PdfCoreResult.Success(true)

    override fun saveCompressed(preset: CompressPreset, signaturesAcknowledged: Boolean): PdfCoreResult<CompressedCopy> =
        if (signaturesAcknowledged) document.saveCompressed(preset, true) else refused

    override fun protectionWillInvalidateSignatures(): PdfCoreResult<Boolean> = PdfCoreResult.Success(true)

    override fun protect(openPassword: String, permissionsPassword: String, signaturesAcknowledged: Boolean): PdfCoreResult<ByteArray> =
        if (signaturesAcknowledged) document.protect(openPassword, permissionsPassword, true) else refused

    override fun recoverySnapshot(): PdfCoreResult<RecoverySnapshot> = when (val taken = document.recoverySnapshot()) {
        is PdfCoreResult.Success -> PdfCoreResult.Success(RecoverySnapshot(taken.value.bytes, breaksSignatures = true))
        is PdfCoreResult.Failure -> taken
    }
}
