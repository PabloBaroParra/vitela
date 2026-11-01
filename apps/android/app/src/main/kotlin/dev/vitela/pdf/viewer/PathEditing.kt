package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.ContentPath
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * The vector half of Edit content: resizing, moving or deleting the table
 * border, rule or shaded box the open shape dialog holds. The same shape as
 * [ImageEditing], which it copies — [content] owns the mode, routes the page
 * tap that places an armed move, and lands each accepted edit.
 */
internal class PathEditing(
    private val session: ViewerSession,
    private val content: ContentEditing,
) {
    private val state = session.state

    fun dismissShaper() {
        val mode = state.value.contentEdit ?: return
        if (mode.shaper != null) state.value = state.value.copy(contentEdit = mode.copy(shaper = null))
    }

    /**
     * Swaps the open shape dialog for an armed move of its path, for the
     * document [documentId] the dialog was built for: the next page tap places it.
     */
    fun armMove(documentId: Long) {
        if (documentId != state.value.documentId) return
        val mode = state.value.contentEdit ?: return
        val path = mode.shaper?.path ?: return
        state.value = state.value.copy(contentEdit = mode.copy(shaper = null, moving = ContentTarget.Path(path)), status = pathMovePrompt(path.pageIndex))
    }

    /**
     * Reshapes the open dialog's path to [width] by [height] points, as typed,
     * for the document [documentId] the dialog was built for. A size that
     * cannot be sent keeps the dialog open with what was typed; an axis the
     * path has none of is not read.
     */
    fun resize(documentId: Long, width: String, height: String) {
        val openDocument = session.document ?: return
        if (documentId != state.value.documentId) return
        val mode = state.value.contentEdit ?: return
        val shaper = mode.shaper ?: return
        val path = shaper.path
        val to = resizedPathRect(path.bounds, typedPoints(width), typedPoints(height))
        if (to == null) {
            state.value = state.value.copy(contentEdit = mode.copy(shaper = shaper.copy(width = width, height = height, error = PATH_SIZE_INVALID)))
            return
        }
        if (to == path.bounds) {
            dismissShaper()
            return
        }
        session.scope.launch {
            session.documentLane.withLock {
                // Only the dialog this came from, still open: a second tap on
                // Resize before the core answered the first finds it answered.
                if (session.document !== openDocument || state.value.contentEdit?.shaper !== shaper) return@withLock
                when (val result = withContext(session.compute) { openDocument.resizePath(path, to) }) {
                    // The outline follows the core's word for the box the path now fills.
                    is PdfCoreResult.Success -> content.landed(openDocument, path.pageIndex, PATH_RESIZED, ::dismissShaper)
                    is PdfCoreResult.Failure -> {
                        val now = state.value.contentEdit ?: return@withLock
                        if (now.shaper?.path != path) return@withLock
                        state.value = state.value.copy(contentEdit = now.copy(shaper = shaper.copy(width = width, height = height, error = userMessage(result.error))))
                    }
                }
            }
        }
    }

    /**
     * Deletes the open dialog's path, for the document [documentId] the dialog
     * was built for. The dialog is spent before the core answers, like an
     * image's: a second tap on Delete finds nothing open. A refusal is
     * reported in the status line — there is nothing typed to keep.
     */
    fun delete(documentId: Long) {
        val openDocument = session.document ?: return
        if (documentId != state.value.documentId) return
        val mode = state.value.contentEdit ?: return
        val path = mode.shaper?.path ?: return
        state.value = state.value.copy(contentEdit = mode.copy(shaper = null))
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit == null) return@withLock
                when (val result = withContext(session.compute) { openDocument.removePath(path) }) {
                    // The core no longer reports the path, so its outline goes too.
                    is PdfCoreResult.Success -> content.landed(openDocument, path.pageIndex, PATH_DELETED)
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }

    /**
     * Moves [path] so its top-left corner lands on the tap at [point]. A tap on
     * another page moves nothing — `MovePath` carries a box, not a page — and
     * leaves the move armed. Spent before the core answers, like an image's.
     */
    fun move(openDocument: PdfDocument, path: ContentPath, pageIndex: Int, point: AnnotationPoint) {
        if (pageIndex != path.pageIndex) {
            state.value = state.value.copy(status = pathMovePrompt(path.pageIndex))
            return
        }
        val mode = state.value.contentEdit ?: return
        val to = movedRect(path.bounds, point)
        state.value = state.value.copy(
            contentEdit = mode.copy(moving = null),
            status = if (to == path.bounds) PATH_POSITION_UNCHANGED else state.value.status,
        )
        if (to == path.bounds) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit == null) return@withLock
                when (val result = withContext(session.compute) { openDocument.movePath(path, to) }) {
                    // The outline follows the core's word for where the path now is.
                    is PdfCoreResult.Success -> content.landed(openDocument, path.pageIndex, PATH_MOVED)
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }
}
