package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.SaveSnapshot
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Serializes the open document for Save, Save copy, and Print. The shell
 * writes the bytes (or rasterizes the print copy); this owns the snapshot and whether a finished write
 * still belongs to the document on screen.
 */
internal class DocumentSaving(private val session: ViewerSession) {
    private val state = session.state

    /**
     * A throwaway copy of the document for the print job to rasterize: the
     * core's output snapshot (every applied edit, annotations included) rather
     * than the live document, whose render preview leaves annotations out.
     * The core reopens it under the password it already holds, so an
     * encrypted document prints too. The caller owns it and must close it.
     * Null, with the reason in the status, when there is nothing to print or
     * the snapshot fails — never a fallback to the source file, which
     * predates the edits.
     */
    suspend fun printDocument(): PdfDocument? = session.documentLane.withLock {
        val openDocument = session.document ?: return@withLock null
        when (val result = withContext(session.compute) { openDocument.outputSnapshot() }) {
            is PdfCoreResult.Success -> result.value
            is PdfCoreResult.Failure -> {
                state.value = state.value.copy(status = userMessage(result.error))
                null
            }
        }
    }

    suspend fun saveSnapshot(): SaveSnapshot? = session.documentLane.withLock {
        val openDocument = session.document ?: return@withLock null
        when (val result = withContext(session.compute) { openDocument.saveToBytes() }) {
            is PdfCoreResult.Success -> SaveSnapshot(result.value, state.value.documentId, state.value.revision)
            is PdfCoreResult.Failure -> {
                state.value = state.value.copy(status = userMessage(result.error))
                null
            }
        }
    }

    fun confirmSaved(snapshot: SaveSnapshot) {
        session.scope.launch {
            session.documentLane.withLock {
                if (state.value.matches(snapshot)) {
                    state.value = state.value.copy(isDirty = false, status = "Saved.")
                }
            }
        }
    }

    fun reportSaveFailure() {
        state.value = state.value.copy(status = "Could not save the PDF.")
    }

    /** A snapshot for **Save**, or null when the open document has nowhere writable to go back to. */
    suspend fun inPlaceSave(): InPlaceSave? {
        // Checked again under the lane below; this only skips a pointless snapshot.
        if (state.value.saveTarget == null) return null
        return session.documentLane.withLock {
            val target = state.value.saveTarget ?: return@withLock null
            val openDocument = session.document ?: return@withLock null
            when (val result = withContext(session.compute) { openDocument.saveToBytes() }) {
                is PdfCoreResult.Success -> InPlaceSave(target, SaveSnapshot(result.value, state.value.documentId, state.value.revision))
                is PdfCoreResult.Failure -> {
                    state.value = state.value.copy(status = userMessage(result.error))
                    null
                }
            }
        }
    }

    /**
     * The write-back failed. The target is dropped so **Save** stops offering
     * a write that will only fail again — but only if it still belongs to the
     * open document; a late failure from a replaced one must not disarm this one.
     */
    fun reportInPlaceSaveFailure(save: InPlaceSave) {
        val current = state.value
        if (current.documentId != save.snapshot.documentId || current.saveTarget != save.target) {
            reportSaveFailure()
            return
        }
        state.value = current.copy(saveTarget = null, status = "Could not save to the original file. Use Save copy instead.")
    }
}
