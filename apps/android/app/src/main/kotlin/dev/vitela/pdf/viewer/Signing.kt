package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.SigningCertificate
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/** Writes the signed PDF to the file the user picked; false when it could not. */
typealias SignedPdfWriter = (bytes: ByteArray) -> Boolean

/** Unlocks a certificate file with its password. Supplied by the ViewModel, which owns the core. */
internal typealias CertificateOpener = (bytes: ByteArray, password: String) -> PdfCoreResult<SigningCertificate>

/**
 * Replaces the open session with the signed [bytes]; the failure when it
 * could not. Supplied by the ViewModel, which owns opening, and called with
 * the document lane held.
 */
internal typealias SignedReopener = suspend (displayName: String, bytes: ByteArray, saveTarget: String?) -> PdfCoreError?

/**
 * Sign with a `.pfx`/`.p12` certificate file — the Linux shell's flow, one
 * dialog step at a time: pick the file, unlock it, pick who to sign as; then
 * a destination, and the signed file reopened, as after Protect.
 *
 * Unsaved changes refuse the dialog, as on Linux: a signature vouches for a
 * file, and the one the user has is the one they saved.
 *
 * The unlocked certificate holds private keys, so it lives here and never in
 * the published state, and is closed the moment the signing is over —
 * signed, refused, cancelled or dismissed.
 */
internal class Signing(
    private val session: ViewerSession,
    private val openCertificate: CertificateOpener,
    private val reopen: SignedReopener,
) {
    private val state = session.state

    /** The picked file's bytes, held only until its password is accepted. */
    private var certificateBytes: ByteArray? = null
    /** The unlocked file, while the dialog lists its identities. */
    private var certificate: SigningCertificate? = null
    /** Bumped by every pick and dismissal, so a late unlock never lands in a dialog that moved on. */
    private var attempt = 0

    /** The accepted identity, waiting while the save picker is up; tied to the document it was chosen for. */
    private var pending: SignRequest? = null

    fun open() {
        val openDocument = session.document ?: return
        if (openDocument.pageCount == 0) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val refusal = if (state.value.isDirty) SAVE_BEFORE_SIGNING
                else withContext(session.compute) { openDocument.signingRefusal() }?.let(::signRefusalSentence)
                state.value = state.value.copy(sign = SignEditor(refusal = refusal))
            }
        }
    }

    /** A certificate file was picked: its bytes wait for the password the dialog asks next. */
    fun chooseCertificate(name: String, bytes: ByteArray) {
        state.value.sign?.takeIf { it.refusal == null } ?: return
        forgetCertificate()
        certificateBytes = bytes
        state.value = state.value.copy(sign = SignEditor(certificateName = name))
    }

    fun unlock(password: String) {
        val bytes = certificateBytes ?: return
        val editor = state.value.sign?.takeIf { !it.unlocking } ?: return
        val unlocking = ++attempt
        state.value = state.value.copy(sign = editor.copy(unlocking = true, error = null))
        session.scope.launch {
            val result = withContext(session.compute) { openCertificate(bytes, password) }
            if (unlocking != attempt) {
                (result as? PdfCoreResult.Success)?.value?.close()
                return@launch
            }
            val current = state.value.sign ?: return@launch
            state.value = state.value.copy(
                sign = when (result) {
                    is PdfCoreResult.Failure -> current.copy(unlocking = false, error = CERTIFICATE_LOCKED)
                    is PdfCoreResult.Success -> {
                        val identities = result.value.identities
                        if (identities.isEmpty()) {
                            result.value.close()
                            current.copy(unlocking = false, error = CERTIFICATE_CANNOT_SIGN)
                        } else {
                            certificate = result.value
                            certificateBytes = null
                            current.copy(unlocking = false, identities = identities, selectedIdentityId = identities.first().id, error = null)
                        }
                    }
                },
            )
        }
    }

    fun selectIdentity(id: String) {
        val editor = state.value.sign ?: return
        if (editor.identities.none { it.id == id }) return
        state.value = state.value.copy(sign = editor.copy(selectedIdentityId = id))
    }

    /**
     * Accepts the chosen identity and closes the dialog. A name for the save
     * picker means the signing waits for a destination; null means there was
     * nothing to confirm.
     */
    fun confirm(): String? {
        val openDocument = session.document ?: return null
        val editor = state.value.sign?.takeIf { it.refusal == null } ?: return null
        val identityId = editor.selectedIdentityId ?: return null
        val unlocked = certificate ?: return null
        if (state.value.signRunning) return null
        certificate = null
        pending = SignRequest(openDocument, unlocked, identityId)
        state.value = state.value.copy(sign = null, status = "Choose where to write the signed PDF.")
        return signedFileName(state.value.title)
    }

    /** The picker was dismissed: nothing is written and the certificate is closed. */
    fun cancel() {
        pending?.certificate?.close()
        pending = null
        state.value = state.value.copy(status = "Signing cancelled. No file was written.")
    }

    fun dismiss() {
        forgetCertificate()
        if (state.value.sign != null) state.value = state.value.copy(sign = null)
    }

    /**
     * Signs, hands the bytes to [write], then reopens what was written as
     * [displayName], with [saveTarget] as where **Save** goes next. The
     * certificate is closed either way.
     */
    suspend fun write(displayName: String, saveTarget: String?, write: SignedPdfWriter) {
        val request = pending ?: return
        pending = null
        try {
            session.documentLane.withLock {
                if (session.document !== request.document || state.value.isDirty) {
                    state.value = state.value.copy(status = "The document changed before it could be signed. No file was written.")
                    return
                }
                state.value = state.value.copy(signRunning = true, status = "Signing PDF...")
                try {
                    val bytes = when (val result = withContext(session.compute) { request.document.sign(request.certificate, request.identityId) }) {
                        is PdfCoreResult.Failure -> return report(userMessage(result.error))
                        is PdfCoreResult.Success -> result.value
                    }
                    if (!withContext(session.io) { write(bytes) }) return report("Could not write the signed PDF.")
                    report(
                        when (val failure = reopen(displayName, bytes, saveTarget)) {
                            null -> "Signed PDF saved and reopened."
                            PdfCoreError.PasswordRequired, PdfCoreError.WrongPassword -> "Signed PDF saved. Enter its password to reopen it."
                            else -> "The signed PDF was written, but it could not be reopened. ${userMessage(failure)}"
                        },
                    )
                } finally {
                    if (state.value.signRunning) state.value = state.value.copy(signRunning = false)
                }
            }
        } finally {
            request.certificate.close()
        }
    }

    private fun forgetCertificate() {
        attempt++
        certificateBytes = null
        certificate?.close()
        certificate = null
    }

    private fun report(status: String) {
        state.value = state.value.copy(status = status)
    }

    private class SignRequest(val document: PdfDocument, val certificate: SigningCertificate, val identityId: String)
}
