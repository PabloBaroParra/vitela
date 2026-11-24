package dev.vitela.pdf.viewer

import dev.vitela.pdf.R
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.text.UiText
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Writes one part of a split into the folder the user picked; false when it
 * could not. The shell supplies it, so the ViewModel never sees a SAF URI.
 */
typealias SplitPartWriter = (fileName: String, bytes: ByteArray) -> Boolean

/**
 * Split into several PDFs: cuts, then a plan, then one new PDF per part. The
 * order is [PageExtracting]'s and the Windows shell's — the cuts and both
 * permission gates are checked while the dialog is still open, so a refusal is
 * fixed where it was typed instead of after a folder has been picked. The open
 * document is never changed: every part is the core pruning a clone of it.
 */
internal class PageSplitting(private val session: ViewerSession) {
    private val state = session.state

    /** The checked plan waiting for a folder, tied to the document it was made for. */
    private var pending: PendingSplit? = null

    fun open() {
        val openDocument = session.document ?: return
        if (openDocument.pageCount < 2) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val refusal = withContext(session.compute) { pageSplitRefusal(openDocument) }
                state.value = state.value.copy(pageSplit = PageSplitEditor("", refusal == null, refusal))
            }
        }
    }

    fun edit(cuts: String) {
        val editor = state.value.pageSplit ?: return
        if (editor.splitAllowed) state.value = state.value.copy(pageSplit = editor.copy(cuts = cuts, message = null))
    }

    fun dismiss() {
        if (state.value.pageSplit != null) state.value = state.value.copy(pageSplit = null)
    }

    /**
     * Checks the dialog's cuts. True means a plan is waiting for a folder and
     * the dialog is closed; false leaves it open with the reason under the field.
     */
    suspend fun plan(): Boolean {
        val openDocument = session.document ?: return false
        val editor = state.value.pageSplit?.takeIf { it.splitAllowed } ?: return false
        return session.documentLane.withLock {
            if (session.document !== openDocument || state.value.pageSplit == null) return@withLock false
            val title = state.value.title
            when (val result = withContext(session.compute) { planPageSplit(openDocument, title, editor.cuts) }) {
                is PdfCoreResult.Success -> {
                    pending = PendingSplit(result.value, state.value.documentId)
                    state.value = state.value.copy(pageSplit = null)
                    true
                }
                is PdfCoreResult.Failure -> {
                    // Only the message changes: whatever was typed while the plan waited stays.
                    val latest = state.value.pageSplit ?: return@withLock false
                    state.value = state.value.copy(pageSplit = latest.copy(message = dialogMessage(result.error)))
                    false
                }
            }
        }
    }

    /** The picker was dismissed: nothing is written and the plan is dropped. */
    fun cancel() {
        pending = null
        state.value = state.value.copy(status = UiText.Resource(R.string.status_split_cancelled))
    }

    /**
     * Extracts and writes each planned part, stopping at the first failure and
     * saying how many were written — the Windows rule, so a folder missing its
     * middle part is never reported as a finished split. Each part takes the
     * document lane on its own, so a long split does not freeze editing.
     */
    suspend fun split(write: SplitPartWriter) {
        val split = pending ?: return
        pending = null
        if (state.value.pageSplitRunning) return
        val parts = split.plan.parts
        state.value = state.value.copy(pageSplitRunning = true)
        var written = 0
        try {
            for (part in parts) {
                state.value = state.value.copy(status = UiText.Resource(R.string.status_split_busy, written + 1, parts.size))
                val extracted = session.documentLane.withLock {
                    val openDocument = session.document
                    if (openDocument == null || state.value.documentId != split.documentId) return@withLock null
                    withContext(session.compute) { openDocument.extractPages((part.first..part.last).toList()) }
                }
                val bytes = when (extracted) {
                    null -> return finish(UiText.Plural(R.plurals.status_split_document_changed, written, written))
                    is PdfCoreResult.Failure -> return finish(UiText.Plural(R.plurals.status_split_failed, written, written, userMessage(extracted.error)))
                    is PdfCoreResult.Success -> extracted.value
                }
                if (!withContext(session.io) { write(part.fileName, bytes) }) {
                    return finish(UiText.Plural(R.plurals.status_split_write_failed, written, written, part.fileName))
                }
                written++
            }
            finish(pageSplitSummary(written, split.plan.sourceIsSigned))
        } finally {
            // A cancelled split (the screen went away) must not leave Split disabled.
            if (state.value.pageSplitRunning) state.value = state.value.copy(pageSplitRunning = false)
        }
    }

    private fun finish(status: UiText) {
        state.value = state.value.copy(pageSplitRunning = false, status = status)
    }

    private class PendingSplit(val plan: PageSplitPlan, val documentId: Long)
}
