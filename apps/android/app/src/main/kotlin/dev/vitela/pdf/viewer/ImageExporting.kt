package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreResult
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Writes one exported page into the folder the user picked; false when it could
 * not. The shell supplies it, so the ViewModel never sees a SAF URI.
 */
typealias ImageFileWriter = (fileName: String, mimeType: String, bytes: ByteArray) -> Boolean

/**
 * Export images: choices, then a plan, then one file per page. The order is
 * the Windows shell's — every choice is checked while the dialog is still open,
 * so a bad range or an oversized page is fixed where it was typed instead of
 * being discovered after a folder has been picked.
 */
internal class ImageExporting(private val session: ViewerSession) {
    private val state = session.state

    /** The checked plan waiting for a folder, tied to the document it was made for. */
    private var pending: PendingExport? = null

    fun open() {
        val openDocument = session.document ?: return
        if (openDocument.pageCount == 0) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val allowed = withContext(Dispatchers.Default) { openDocument.imageExportAllowed() }
                state.value = state.value.copy(
                    imageExport = ImageExportEditor(ImageExportDraft(), allowed, if (allowed) null else IMAGE_EXPORT_NOT_ALLOWED),
                )
            }
        }
    }

    fun edit(draft: ImageExportDraft) {
        val editor = state.value.imageExport ?: return
        if (editor.exportAllowed) state.value = state.value.copy(imageExport = editor.copy(draft = draft, message = null))
    }

    fun dismiss() {
        if (state.value.imageExport != null) state.value = state.value.copy(imageExport = null)
    }

    /**
     * Checks the dialog's choices. True means a plan is waiting for a folder and
     * the dialog is closed; false leaves it open with the reason under the fields.
     */
    suspend fun plan(): Boolean {
        val openDocument = session.document ?: return false
        val editor = state.value.imageExport?.takeIf { it.exportAllowed } ?: return false
        return session.documentLane.withLock {
            if (session.document !== openDocument || state.value.imageExport == null) return@withLock false
            val current = state.value.pageIndex
            val title = state.value.title
            when (val result = withContext(Dispatchers.Default) { planImageExport(openDocument, title, current, editor.draft) }) {
                is PdfCoreResult.Success -> {
                    pending = PendingExport(result.value, state.value.documentId)
                    state.value = state.value.copy(imageExport = null)
                    true
                }
                is PdfCoreResult.Failure -> {
                    // Only the message changes: whatever was typed while the plan waited stays.
                    val latest = state.value.imageExport ?: return@withLock false
                    state.value = state.value.copy(imageExport = latest.copy(message = userMessage(result.error)))
                    false
                }
            }
        }
    }

    /** The picker was dismissed: nothing is written and the plan is dropped. */
    fun cancel() {
        pending = null
        state.value = state.value.copy(status = "Export cancelled. No file was written.")
    }

    /**
     * Renders and writes each planned page, stopping at the first failure.
     * Stopping is the point: a missing file in a folder of hundreds goes
     * unnoticed until someone needs it, so what *was* written is counted
     * instead of carrying on. Each page takes the document lane on its own, so
     * an export of a long file does not freeze editing for its whole length.
     */
    suspend fun export(write: ImageFileWriter) {
        val export = pending ?: return
        pending = null
        if (state.value.imageExportRunning) return
        val plan = export.plan
        state.value = state.value.copy(imageExportRunning = true)
        var written = 0
        try {
            for (file in plan.files) {
                state.value = state.value.copy(status = "Exporting page ${file.pageIndex + 1} (${written + 1} of ${plan.files.size})...")
                val rendered = session.documentLane.withLock {
                    val openDocument = session.document
                    if (openDocument == null || state.value.documentId != export.documentId) return@withLock null
                    withContext(Dispatchers.Default) { openDocument.exportPageImage(file.pageIndex, plan.dpi, plan.format) }
                }
                val image = when (rendered) {
                    null -> return finish("The document changed. Export cancelled.")
                    is PdfCoreResult.Failure ->
                        return finish("Page ${file.pageIndex + 1} could not be exported: ${userMessage(rendered.error)} $written written.")
                    is PdfCoreResult.Success -> rendered.value
                }
                if (!withContext(Dispatchers.IO) { write(file.fileName, plan.format.mimeType, image) }) {
                    return finish("Could not write ${file.fileName}. $written written.")
                }
                written++
            }
            finish(imageExportSummary(written, plan.format))
        } finally {
            // A cancelled export (the screen went away) must not leave Export disabled.
            if (state.value.imageExportRunning) state.value = state.value.copy(imageExportRunning = false)
        }
    }

    private fun finish(status: String) {
        state.value = state.value.copy(imageExportRunning = false, status = status)
    }

    private class PendingExport(val plan: ImageExportPlan, val documentId: Long)
}
