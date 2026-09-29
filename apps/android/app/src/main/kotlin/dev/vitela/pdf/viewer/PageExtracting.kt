package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreResult
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Writes the extracted PDF to the file the user picked; false when it could
 * not. The shell supplies it, so the ViewModel never sees a SAF URI.
 */
typealias ExtractedPdfWriter = (bytes: ByteArray) -> Boolean

/**
 * Extract pages: a range, then a plan, then one new PDF. The same order as
 * [ImageExporting] and the Windows shell — the range and both permission gates
 * are checked while the dialog is still open, so a refusal is fixed where it
 * was typed instead of after a file has been created. The open document is
 * never changed: the core prunes a clone of it.
 */
internal class PageExtracting(private val session: ViewerSession) {
    private val state = session.state

    /** The checked plan waiting for a destination, tied to the document it was made for. */
    private var pending: PendingExtract? = null

    fun open() {
        val openDocument = session.document ?: return
        if (openDocument.pageCount == 0) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val refusal = withContext(session.compute) { pageExtractRefusal(openDocument) }
                state.value = state.value.copy(pageExtract = PageExtractEditor("", refusal == null, refusal))
            }
        }
    }

    fun edit(range: String) {
        val editor = state.value.pageExtract ?: return
        if (editor.extractAllowed) state.value = state.value.copy(pageExtract = editor.copy(range = range, message = null))
    }

    fun dismiss() {
        if (state.value.pageExtract != null) state.value = state.value.copy(pageExtract = null)
    }

    /**
     * Checks the dialog's range. A name for the save picker means a plan is
     * waiting for a destination and the dialog is closed; null leaves it open
     * with the reason under the field.
     */
    suspend fun plan(): String? {
        val openDocument = session.document ?: return null
        val editor = state.value.pageExtract?.takeIf { it.extractAllowed } ?: return null
        return session.documentLane.withLock {
            if (session.document !== openDocument || state.value.pageExtract == null) return@withLock null
            when (val result = withContext(session.compute) { planPageExtract(openDocument, editor.range) }) {
                is PdfCoreResult.Success -> {
                    pending = PendingExtract(result.value, state.value.documentId)
                    state.value = state.value.copy(pageExtract = null)
                    extractFileName(state.value.title)
                }
                is PdfCoreResult.Failure -> {
                    // Only the message changes: whatever was typed while the plan waited stays.
                    val latest = state.value.pageExtract ?: return@withLock null
                    state.value = state.value.copy(pageExtract = latest.copy(message = userMessage(result.error)))
                    null
                }
            }
        }
    }

    /** The picker was dismissed: nothing is written and the plan is dropped. */
    fun cancel() {
        pending = null
        state.value = state.value.copy(status = "Extract cancelled. No file was written.")
    }

    /** Asks the core for the extracted PDF and hands it to [write]. The plan is spent either way. */
    suspend fun extract(write: ExtractedPdfWriter) {
        val extract = pending ?: return
        pending = null
        val plan = extract.plan
        val count = plan.pages.size
        state.value = state.value.copy(status = "Extracting $count ${if (count == 1) "page" else "pages"}...")
        val result = session.documentLane.withLock {
            val openDocument = session.document
            if (openDocument == null || state.value.documentId != extract.documentId) return@withLock null
            withContext(session.compute) { openDocument.extractPages(plan.pages) }
        }
        val bytes = when (result) {
            null -> return report("The document changed. Extract cancelled.")
            is PdfCoreResult.Failure -> return report(userMessage(result.error))
            is PdfCoreResult.Success -> result.value
        }
        if (!withContext(session.io) { write(bytes) }) return report("Could not write the extracted PDF.")
        report(pageExtractSummary(count, plan.sourceIsSigned))
    }

    private fun report(status: String) {
        state.value = state.value.copy(status = status)
    }

    private class PendingExtract(val plan: PageExtractPlan, val documentId: Long)
}
