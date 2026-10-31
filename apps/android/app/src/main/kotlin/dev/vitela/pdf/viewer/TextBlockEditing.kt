package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.ContentTextBlock
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * The text half of Edit content: retyping, moving or deleting the block — a
 * line of runs — the open editor holds. Every edit goes through the block
 * edits, a one-run block included: once a block is retyped its first run's
 * edit lives inside a batch, and a per-run edit would no longer see it.
 * [content] owns the mode, routes the page tap that places an armed move, and
 * lands each accepted edit.
 */
internal class TextBlockEditing(
    private val session: ViewerSession,
    private val content: ContentEditing,
) {
    private val state = session.state

    fun dismissEditor() {
        val mode = state.value.contentEdit ?: return
        if (mode.editor != null) state.value = state.value.copy(contentEdit = mode.copy(editor = null))
    }

    /**
     * Retypes the open editor's block as [text], for the document [documentId]
     * the dialog was built for: a dialog confirmed after another document
     * replaced it holds a block that means nothing against the new file.
     */
    fun retype(documentId: Long, text: String) {
        val openDocument = session.document ?: return
        if (documentId != state.value.documentId) return
        val editor = state.value.contentEdit?.editor ?: return
        if (text == editor.block.text) {
            dismissEditor()
            return
        }
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                when (val result = withContext(session.compute) { openDocument.retypeTextBlock(editor.block, text) }) {
                    // The line is wider or narrower now, and may be one run where it was several; the outline follows the core's word for it.
                    is PdfCoreResult.Success -> content.landed(openDocument, editor.block.pageIndex, TEXT_UPDATED, ::dismissEditor)
                    is PdfCoreResult.Failure -> {
                        val mode = state.value.contentEdit ?: return@withLock
                        if (mode.editor?.block != editor.block) return@withLock
                        state.value = state.value.copy(contentEdit = mode.copy(editor = editor.copy(text = text, error = userMessage(result.error))))
                    }
                }
            }
        }
    }

    /**
     * Deletes the open editor's block, for the document [documentId] the dialog
     * was built for. Spent before the core answers, like an image delete: a
     * second tap on Delete finds nothing open, and a refusal is reported in
     * the status line.
     */
    fun deleteText(documentId: Long) {
        val openDocument = session.document ?: return
        if (documentId != state.value.documentId) return
        val mode = state.value.contentEdit ?: return
        val block = mode.editor?.block ?: return
        state.value = state.value.copy(contentEdit = mode.copy(editor = null))
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit == null) return@withLock
                when (val result = withContext(session.compute) { openDocument.removeTextBlock(block) }) {
                    // The core no longer reports the block, so its outline goes too.
                    is PdfCoreResult.Success -> content.landed(openDocument, block.pageIndex, TEXT_DELETED)
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }

    /**
     * Swaps the open editor for an armed move of its block, for the document
     * [documentId] the dialog was built for: the next page tap places it.
     */
    fun armMove(documentId: Long) {
        if (documentId != state.value.documentId) return
        val mode = state.value.contentEdit ?: return
        val block = mode.editor?.block ?: return
        state.value = state.value.copy(contentEdit = mode.copy(editor = null, moving = ContentTarget.Block(block)), status = textMovePrompt(block.pageIndex))
    }

    /**
     * Moves [block] so its top-left corner lands on the tap at [point], keeping
     * its text, font and size: every run goes by the same offset, one undo
     * step. A tap on another page moves nothing — a block move carries an
     * offset, not a page — and leaves the move armed. Spent before the core
     * answers, like an image's move: one tap, one edit.
     */
    fun move(openDocument: PdfDocument, block: ContentTextBlock, pageIndex: Int, point: AnnotationPoint) {
        if (pageIndex != block.pageIndex) {
            state.value = state.value.copy(status = textMovePrompt(block.pageIndex))
            return
        }
        val mode = state.value.contentEdit ?: return
        val to = movedRect(block.bounds, point)
        val dx = to.x - block.bounds.x
        val dy = to.y - block.bounds.y
        val unchanged = dx == 0.0 && dy == 0.0
        state.value = state.value.copy(
            contentEdit = mode.copy(moving = null),
            status = if (unchanged) TEXT_POSITION_UNCHANGED else state.value.status,
        )
        if (unchanged) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit == null) return@withLock
                when (val result = withContext(session.compute) { openDocument.moveTextBlock(block, dx, dy) }) {
                    // The outline follows the core's word for where the block now is.
                    is PdfCoreResult.Success -> content.landed(openDocument, block.pageIndex, TEXT_MOVED)
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }
}
