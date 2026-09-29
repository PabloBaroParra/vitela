package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.DocumentInfo
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * The Document properties dialog. An applied change is one entry in the
 * shared edit log, so [annotations] refreshes afterwards to light up Undo.
 */
internal class MetadataEditing(private val session: ViewerSession, private val annotations: AnnotationEditing) {
    private val state = session.state

    /** Opens Document properties on the `/Info` dict as it would be saved now. */
    fun open() {
        val openDocument = session.document ?: return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val allowed = withContext(Dispatchers.Default) { openDocument.metadataEditingAllowed() }
                when (val result = withContext(Dispatchers.Default) { openDocument.documentInfo() }) {
                    is PdfCoreResult.Success -> state.value = state.value.copy(
                        metadataEditor = MetadataEditor(result.value, allowed, if (allowed) null else METADATA_READ_ONLY),
                    )
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }

    fun edit(draft: DocumentInfo) {
        val editor = state.value.metadataEditor ?: return
        if (editor.editingAllowed) state.value = state.value.copy(metadataEditor = editor.copy(draft = draft))
    }

    fun dismiss() {
        if (state.value.metadataEditor != null) state.value = state.value.copy(metadataEditor = null)
    }

    /**
     * Queues the draft as one undoable change. The current value is read again
     * under the lane, not trusted from when the dialog opened: an undo may
     * have landed in between.
     */
    fun apply() {
        val openDocument = session.document ?: return
        val editor = state.value.metadataEditor?.takeIf { it.editingAllowed } ?: return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.metadataEditor == null) return@withLock
                val current = when (val read = withContext(Dispatchers.Default) { openDocument.documentInfo() }) {
                    is PdfCoreResult.Success -> read.value
                    is PdfCoreResult.Failure -> return@withLock refuse(read.error)
                }
                val after = metadataChange(current, editor.draft) ?: run {
                    state.value = state.value.copy(metadataEditor = null)
                    return@withLock
                }
                when (val result = withContext(Dispatchers.Default) { openDocument.setDocumentInfo(after) }) {
                    is PdfCoreResult.Success -> {
                        state.value = state.value.copy(
                            metadataEditor = null,
                            isDirty = true,
                            revision = state.value.revision + 1,
                            status = "Document properties updated. Changes are pending save.",
                        )
                        // The change sits in the same edit log: Undo must light up.
                        annotations.refresh(openDocument)
                    }
                    is PdfCoreResult.Failure -> refuse(result.error)
                }
            }
        }
    }

    /** Only the message changes: whatever was typed while the apply waited stays. */
    private fun refuse(error: PdfCoreError) {
        val editor = state.value.metadataEditor ?: return
        state.value = state.value.copy(metadataEditor = editor.copy(message = userMessage(error)))
    }
}
