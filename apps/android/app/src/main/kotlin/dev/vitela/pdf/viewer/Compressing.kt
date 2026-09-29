package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.CompressPreset
import dev.vitela.pdf.core.CompressedCopy
import dev.vitela.pdf.core.PdfCoreResult
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Writes the compressed PDF to the file the user picked; false when it could
 * not. The shell supplies it, so the ViewModel never sees a SAF URI.
 */
typealias CompressedPdfWriter = (bytes: ByteArray) -> Boolean

/**
 * Compress: a preset, then the run, then a destination — the Windows order.
 * Both gates (a document that cannot be compressed, a signature that would
 * break) are asked while the dialog opens, so the dialog can say why before
 * anything runs. The destination comes *last* because the core promises the
 * copy is never larger, not that it is ever smaller: when nothing smaller
 * came out, no picker opens at all.
 *
 * Nothing is reopened afterwards. The open session, its pending edits and its
 * undo history are exactly as they were; there is simply a smaller copy.
 */
internal class Compressing(private val session: ViewerSession) {
    private val state = session.state

    /**
     * The smaller bytes waiting for a destination. Not tied to a document: they
     * were computed before the picker opened, so a document replaced while it
     * is up does not change what they are — a compressed copy of what was open.
     */
    private var pending: CompressedCopy? = null

    fun open() {
        val openDocument = session.document ?: return
        if (openDocument.pageCount == 0) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val editor = withContext(session.compute) {
                    val refusal = openDocument.compressionRefusal()
                    if (refusal != null) return@withContext CompressEditor(refusal = compressRefusalSentence(refusal))
                    // A failed query refuses rather than guessing "unsigned": a
                    // guess could write a copy whose signature silently broke.
                    when (val signed = openDocument.compressedSaveWillInvalidateSignatures()) {
                        is PdfCoreResult.Success -> CompressEditor(signaturesWillBreak = signed.value)
                        is PdfCoreResult.Failure -> CompressEditor(refusal = userMessage(signed.error))
                    }
                }
                state.value = state.value.copy(compress = editor)
            }
        }
    }

    fun select(preset: CompressPreset) {
        val editor = state.value.compress ?: return
        if (editor.refusal == null) state.value = state.value.copy(compress = editor.copy(preset = preset))
    }

    fun dismiss() {
        if (state.value.compress != null) state.value = state.value.copy(compress = null)
    }

    /**
     * Runs the dialog's preset and closes it. A name for the save picker means
     * smaller bytes are waiting for a destination; null means there is nothing
     * to write, and the status line says why.
     */
    suspend fun compress(): String? {
        val openDocument = session.document ?: return null
        val editor = state.value.compress?.takeIf { it.refusal == null } ?: return null
        if (state.value.compressRunning) return null
        return session.documentLane.withLock {
            if (session.document !== openDocument || state.value.compress == null) return@withLock null
            state.value = state.value.copy(compress = null, compressRunning = true, status = "Compressing PDF...")
            try {
                // The dialog showed the signature warning and its button said so:
                // confirming it is the acknowledgement, never a second prompt.
                val result = withContext(session.compute) { openDocument.saveCompressed(editor.preset, editor.signaturesWillBreak) }
                when (result) {
                    is PdfCoreResult.Failure -> report(userMessage(result.error))
                    is PdfCoreResult.Success -> {
                        val copy = result.value
                        if (!copy.reduced) {
                            report(compressNoGainSummary(copy))
                        } else {
                            pending = copy
                            report(compressReductionSummary(copy))
                            compressedFileName(state.value.title)
                        }
                    }
                }
            } finally {
                if (state.value.compressRunning) state.value = state.value.copy(compressRunning = false)
            }
        }
    }

    /** The picker was dismissed: nothing is written and the bytes are dropped. */
    fun cancel() {
        pending = null
        state.value = state.value.copy(status = "Compression cancelled. No file was written.")
    }

    /** Hands the waiting bytes to [write]. They are spent either way. */
    suspend fun write(write: CompressedPdfWriter) {
        val copy = pending ?: return
        pending = null
        state.value = state.value.copy(status = "Writing the compressed PDF...")
        if (!withContext(session.io) { write(copy.bytes) }) {
            report("Could not write the compressed PDF.")
        } else {
            report(compressWrittenSummary(copy))
        }
    }

    /** Always null, so a branch that only reports can be the value of [compress]. */
    private fun report(status: String): String? {
        state.value = state.value.copy(status = status)
        return null
    }
}
