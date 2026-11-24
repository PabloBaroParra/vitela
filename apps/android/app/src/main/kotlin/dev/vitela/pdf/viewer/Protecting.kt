package dev.vitela.pdf.viewer

import dev.vitela.pdf.R
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.text.UiText
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Writes the protected PDF to the file the user picked; false when it could
 * not. The shell supplies it, so the ViewModel never sees a SAF URI.
 */
typealias ProtectedPdfWriter = (bytes: ByteArray) -> Boolean

/**
 * Replaces the open session with [bytes] opened under both passwords; the
 * failure when it could not. Supplied by the ViewModel, which owns opening,
 * and called with the document lane already held.
 */
internal typealias ProtectedReopener = suspend (displayName: String, bytes: ByteArray, openPassword: String, permissionsPassword: String, saveTarget: String?) -> PdfCoreError?

/**
 * Protect with a password: two passwords, then a destination, then the
 * protected file reopened under both — the Windows and GTK order. Both gates
 * (a document whose protection may not change, a signature that would break)
 * are asked while the dialog opens, so the dialog can say why before anything
 * runs.
 *
 * The reopen is the point, not a courtesy: the session on screen becomes the
 * protected file, opened with both passwords so its next rewrite can re-apply
 * both roles. Pending edits are carried by the protected bytes; undo history
 * is not, exactly as on the other shells.
 */
internal class Protecting(private val session: ViewerSession, private val reopen: ProtectedReopener) {
    private val state = session.state

    /**
     * The accepted passwords, waiting while the save picker is up. Held only
     * that long and tied to the document they were typed for: a document
     * replaced behind the picker is never protected with them.
     */
    private var pending: ProtectRequest? = null

    fun open() {
        val openDocument = session.document ?: return
        if (openDocument.pageCount == 0) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val editor = withContext(session.compute) {
                    if (!openDocument.protectionChangeAllowed()) return@withContext ProtectEditor(refusal = PROTECTION_REFUSED)
                    // A failed query refuses rather than guessing "unsigned": a
                    // guess could write a copy whose signature silently broke.
                    when (val signed = openDocument.protectionWillInvalidateSignatures()) {
                        is PdfCoreResult.Success -> ProtectEditor(signaturesWillBreak = signed.value)
                        is PdfCoreResult.Failure -> ProtectEditor(refusal = dialogMessage(signed.error))
                    }
                }
                state.value = state.value.copy(protect = editor)
            }
        }
    }

    fun dismiss() {
        if (state.value.protect != null) state.value = state.value.copy(protect = null)
    }

    /**
     * Accepts the dialog's passwords and closes it. A name for the save picker
     * means they are waiting for a destination; null leaves the dialog open
     * with the reason, or means there was nothing to confirm.
     */
    fun confirm(openPassword: String, permissionsPassword: String): String? {
        val openDocument = session.document ?: return null
        val editor = state.value.protect?.takeIf { it.refusal == null } ?: return null
        if (state.value.protectRunning) return null
        protectionPasswordProblem(openPassword, permissionsPassword)?.let { problem ->
            state.value = state.value.copy(protect = editor.copy(error = problem))
            return null
        }
        // The dialog showed the signature warning and its button said so:
        // confirming it is the acknowledgement, never a second prompt.
        pending = ProtectRequest(openDocument, openPassword, permissionsPassword, editor.signaturesWillBreak)
        state.value = state.value.copy(protect = null, status = UiText.Resource(R.string.status_protect_choose))
        return protectedFileName(state.value.title)
    }

    /** The picker was dismissed: nothing is written and the passwords are dropped. */
    fun cancel() {
        pending = null
        state.value = state.value.copy(status = UiText.Resource(R.string.status_protect_cancelled))
    }

    /**
     * Protects, hands the bytes to [write], then reopens what was written as
     * [displayName], with [saveTarget] as where **Save** goes next. The
     * passwords are spent either way.
     */
    suspend fun write(displayName: String, saveTarget: String?, write: ProtectedPdfWriter) {
        val request = pending ?: return
        pending = null
        session.documentLane.withLock {
            if (session.document !== request.document) {
                state.value = state.value.copy(status = UiText.Resource(R.string.status_protect_document_changed))
                return
            }
            state.value = state.value.copy(protectRunning = true, status = UiText.Resource(R.string.status_protect_busy))
            try {
                val result = withContext(session.compute) {
                    request.document.protect(request.openPassword, request.permissionsPassword, request.signaturesAcknowledged)
                }
                val bytes = when (result) {
                    is PdfCoreResult.Failure -> return report(userMessage(result.error))
                    is PdfCoreResult.Success -> result.value
                }
                if (!withContext(session.io) { write(bytes) }) return report(UiText.Resource(R.string.status_protect_write_failed))
                val failure = reopen(displayName, bytes, request.openPassword, request.permissionsPassword, saveTarget)
                report(
                    if (failure == null) UiText.Resource(R.string.status_protect_done)
                    else UiText.Sentences(UiText.Resource(R.string.status_protect_not_reopened), userMessage(failure)),
                )
            } finally {
                if (state.value.protectRunning) state.value = state.value.copy(protectRunning = false)
            }
        }
    }

    private fun report(status: UiText) {
        state.value = state.value.copy(status = status)
    }

    private class ProtectRequest(
        val document: PdfDocument,
        val openPassword: String,
        val permissionsPassword: String,
        val signaturesAcknowledged: Boolean,
    )
}
