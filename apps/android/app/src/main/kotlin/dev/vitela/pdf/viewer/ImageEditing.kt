package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * The image half of Edit content: resizing, moving, replacing or deleting the image the
 * open resizer holds. [content] owns the mode, routes the page tap that
 * places an armed move, and lands each accepted edit.
 */
internal class ImageEditing(
    private val session: ViewerSession,
    private val content: ContentEditing,
) {
    private val state = session.state

    fun dismissResizer() {
        val mode = state.value.contentEdit ?: return
        if (mode.resizer != null) state.value = state.value.copy(contentEdit = mode.copy(resizer = null))
    }

    /**
     * Swaps the open resizer for an armed move of its image, for the document
     * [documentId] the dialog was built for: the next page tap places it.
     */
    fun armMove(documentId: Long) {
        if (documentId != state.value.documentId) return
        val mode = state.value.contentEdit ?: return
        val image = mode.resizer?.image ?: return
        state.value = state.value.copy(contentEdit = mode.copy(resizer = null, moving = ContentTarget.Image(image)), status = imageMovePrompt(image.pageIndex))
    }

    /**
     * Resizes the open resizer's image to [width] by [height] points, as typed,
     * for the document [documentId] the dialog was built for. A size that is
     * not a finite, positive number keeps the dialog open with what was typed.
     */
    fun resize(documentId: Long, width: String, height: String) {
        val openDocument = session.document ?: return
        if (documentId != state.value.documentId) return
        val mode = state.value.contentEdit ?: return
        val resizer = mode.resizer ?: return
        val image = resizer.image
        val to = resizedImageRect(image.bounds, typedPoints(width), typedPoints(height))
        if (to == null) {
            state.value = state.value.copy(contentEdit = mode.copy(resizer = resizer.copy(width = width, height = height, error = IMAGE_SIZE_INVALID)))
            return
        }
        if (to == image.bounds) {
            dismissResizer()
            return
        }
        session.scope.launch {
            session.documentLane.withLock {
                // Only the dialog this came from, still open: a second tap on
                // Resize before the core answered the first finds it answered.
                if (session.document !== openDocument || state.value.contentEdit?.resizer !== resizer) return@withLock
                when (val result = withContext(session.compute) { openDocument.resizeImage(image, to) }) {
                    // The outline follows the core's word for the box the image now fills.
                    is PdfCoreResult.Success -> content.landed(openDocument, image.pageIndex, IMAGE_RESIZED, ::dismissResizer)
                    is PdfCoreResult.Failure -> {
                        val now = state.value.contentEdit ?: return@withLock
                        if (now.resizer?.image != image) return@withLock
                        state.value = state.value.copy(contentEdit = now.copy(resizer = resizer.copy(width = width, height = height, error = userMessage(result.error))))
                    }
                }
            }
        }
    }

    /**
     * Deletes the open dialog's image, for the document [documentId] the
     * dialog was built for. The dialog is spent before the core answers, like
     * an armed move: a second tap on Delete finds nothing open. A refusal is
     * reported in the status line — there is nothing typed to keep.
     */
    fun delete(documentId: Long) {
        val openDocument = session.document ?: return
        if (documentId != state.value.documentId) return
        val mode = state.value.contentEdit ?: return
        val image = mode.resizer?.image ?: return
        state.value = state.value.copy(contentEdit = mode.copy(resizer = null))
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit == null) return@withLock
                when (val result = withContext(session.compute) { openDocument.removeImage(image) }) {
                    // The core no longer reports the image, so its outline goes too.
                    is PdfCoreResult.Success -> content.landed(openDocument, image.pageIndex, IMAGE_DELETED)
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }

    /**
     * Asks the core whether the open dialog's image can be replaced, for the
     * document [documentId] the dialog was built for — before a picker opens,
     * like the Windows shell, so a refusal costs no trip to the files. True
     * means the dialog is closed and the picker should open; a refusal is
     * reported in the status line.
     */
    suspend fun prepareReplace(documentId: Long): Boolean {
        val openDocument = session.document ?: return false
        if (documentId != state.value.documentId) return false
        val mode = state.value.contentEdit ?: return false
        val image = mode.resizer?.image ?: return false
        state.value = state.value.copy(contentEdit = mode.copy(resizer = null))
        return session.documentLane.withLock {
            if (session.document !== openDocument) return@withLock false
            val result = withContext(session.compute) { openDocument.prepareImageReplacement(image) }
            val now = state.value.contentEdit ?: return@withLock false
            when (result) {
                is PdfCoreResult.Success -> {
                    state.value = state.value.copy(contentEdit = now.copy(replacingImage = image))
                    true
                }
                is PdfCoreResult.Failure -> {
                    state.value = state.value.copy(status = userMessage(result.error))
                    false
                }
            }
        }
    }

    /** No file came back from the picker, for the reason [status] gives: the image keeps its picture. */
    fun cancelReplace(status: String) {
        val mode = state.value.contentEdit ?: return
        if (mode.replacingImage != null) state.value = state.value.copy(contentEdit = mode.copy(replacingImage = null), status = status)
    }

    /**
     * Replaces the prepared image's picture with the picked file's [bytes].
     * Spent before the core answers, like a delete: a second pick finds
     * nothing prepared. An undo while the picker was open dropped the image,
     * which may no longer be there.
     */
    fun replace(bytes: ByteArray) {
        val openDocument = session.document ?: return
        val mode = state.value.contentEdit ?: return
        val image = mode.replacingImage ?: return
        state.value = state.value.copy(contentEdit = mode.copy(replacingImage = null))
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit == null) return@withLock
                when (val result = withContext(session.compute) { openDocument.replaceImage(image, bytes) }) {
                    is PdfCoreResult.Success -> content.landed(openDocument, image.pageIndex, IMAGE_REPLACED)
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }

    /**
     * Moves [image] so its top-left corner lands on the tap at [point]. A tap
     * on another page moves nothing — `MoveImage` carries a box, not a page —
     * and leaves the move armed. Spent before the core answers, like a form
     * field's move: one tap, one edit, so a second tap before the answer
     * finds nothing armed.
     */
    fun move(openDocument: PdfDocument, image: ContentImage, pageIndex: Int, point: AnnotationPoint) {
        if (pageIndex != image.pageIndex) {
            state.value = state.value.copy(status = imageMovePrompt(image.pageIndex))
            return
        }
        val mode = state.value.contentEdit ?: return
        val to = movedRect(image.bounds, point)
        state.value = state.value.copy(
            contentEdit = mode.copy(moving = null),
            status = if (to == image.bounds) IMAGE_POSITION_UNCHANGED else state.value.status,
        )
        if (to == image.bounds) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit == null) return@withLock
                when (val result = withContext(session.compute) { openDocument.moveImage(image, to) }) {
                    // The outline follows the core's word for where the image now is.
                    is PdfCoreResult.Success -> content.landed(openDocument, image.pageIndex, IMAGE_MOVED)
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }
}
