package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreResult
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * The text half of Edit content: retyping or deleting the run the open
 * editor holds. [content] owns the mode and lands each accepted edit.
 */
internal class TextRunEditing(
    private val session: ViewerSession,
    private val content: ContentEditing,
) {
    private val state = session.state

    fun dismissEditor() {
        val mode = state.value.contentEdit ?: return
        if (mode.editor != null) state.value = state.value.copy(contentEdit = mode.copy(editor = null))
    }

    /**
     * Retypes the open editor's run as [text], for the document [documentId]
     * the dialog was built for: a dialog confirmed after another document
     * replaced it holds a run that means nothing against the new file.
     */
    fun retype(documentId: Long, text: String) {
        val openDocument = session.document ?: return
        if (documentId != state.value.documentId) return
        val editor = state.value.contentEdit?.editor ?: return
        if (text == editor.run.text) {
            dismissEditor()
            return
        }
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                when (val result = withContext(session.compute) { openDocument.retypeTextRun(editor.run, text) }) {
                    // The run is wider or narrower now; the outline follows the core's word for it.
                    is PdfCoreResult.Success -> content.landed(openDocument, editor.run.pageIndex, TEXT_UPDATED, ::dismissEditor)
                    is PdfCoreResult.Failure -> {
                        val mode = state.value.contentEdit ?: return@withLock
                        if (mode.editor?.run != editor.run) return@withLock
                        state.value = state.value.copy(contentEdit = mode.copy(editor = editor.copy(text = text, error = userMessage(result.error))))
                    }
                }
            }
        }
    }

    /**
     * Deletes the open editor's run, for the document [documentId] the dialog
     * was built for. Spent before the core answers, like an image delete: a
     * second tap on Delete finds nothing open, and a refusal is reported in
     * the status line.
     */
    fun deleteText(documentId: Long) {
        val openDocument = session.document ?: return
        if (documentId != state.value.documentId) return
        val mode = state.value.contentEdit ?: return
        val run = mode.editor?.run ?: return
        state.value = state.value.copy(contentEdit = mode.copy(editor = null))
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit == null) return@withLock
                when (val result = withContext(session.compute) { openDocument.removeTextRun(run) }) {
                    // The core no longer reports the run, so its outline goes too.
                    is PdfCoreResult.Success -> content.landed(openDocument, run.pageIndex, TEXT_DELETED)
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }
}
