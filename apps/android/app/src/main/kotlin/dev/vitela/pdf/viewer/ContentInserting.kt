package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * The adding half of Edit content: a new line of text or a new image, painted
 * as page content — not an annotation — with its top-left corner where the
 * reader taps. [content] owns the mode, routes the armed tap here and lands
 * each accepted insert.
 *
 * A tap names the corner, as it does for a moved image, rather than typed
 * coordinates, the Windows shell's dialog: a finger already points at the
 * page. Text needs words and a size, so its tap opens a dialog; an image is
 * sized by the core's placement policy, the one a stamp uses, so its tap is
 * the insert.
 */
internal class ContentInserting(
    private val session: ViewerSession,
    private val content: ContentEditing,
) {
    private val state = session.state

    /** Arms the next page tap to place a new line of text. */
    fun armText() = arm(ContentAddition.Text, TEXT_INSERT_PROMPT)

    /**
     * Arms the next page tap to place the image [bytes]. An image chosen after
     * the mode closed arms nothing: the picker outlived what it was for.
     */
    fun armImage(bytes: ByteArray) = arm(ContentAddition.Image(bytes), IMAGE_INSERT_PROMPT)

    fun cancel() {
        val mode = state.value.contentEdit ?: return
        if (mode.adding != null) state.value = state.value.copy(contentEdit = mode.copy(adding = null), status = INSERT_CANCELLED)
    }

    fun dismissInserter() {
        val mode = state.value.contentEdit ?: return
        if (mode.inserter != null) state.value = state.value.copy(contentEdit = mode.copy(inserter = null))
    }

    /**
     * The armed tap at [point] on page [pageIndex]. Spent at once, like an
     * armed move: one tap, one placement, so a second tap before the core
     * answers finds nothing armed and opens what it hits.
     */
    fun place(openDocument: PdfDocument, addition: ContentAddition, pageIndex: Int, point: AnnotationPoint) {
        val mode = state.value.contentEdit ?: return
        when (addition) {
            ContentAddition.Text -> state.value = state.value.copy(contentEdit = mode.copy(adding = null, inserter = TextInserter(pageIndex, point)))
            is ContentAddition.Image -> {
                state.value = state.value.copy(contentEdit = mode.copy(adding = null))
                insertImage(openDocument, addition.bytes, pageIndex, point)
            }
        }
    }

    /**
     * Inserts the open dialog's line as [text] at [size] points, as typed, for
     * the document [documentId] the dialog was built for. Blank text or a size
     * out of range keeps the dialog open with what was typed.
     */
    fun insertText(documentId: Long, text: String, size: String) {
        val openDocument = session.document ?: return
        if (documentId != state.value.documentId) return
        val mode = state.value.contentEdit ?: return
        val inserter = mode.inserter ?: return
        val bounds = insertedTextRect(inserter.at, typedPoints(size))
        val invalid = when {
            !insertableText(text) -> TEXT_INSERT_EMPTY
            bounds == null -> TEXT_SIZE_INVALID
            else -> null
        }
        if (invalid != null || bounds == null) {
            state.value = state.value.copy(contentEdit = mode.copy(inserter = inserter.copy(text = text, size = size, error = invalid)))
            return
        }
        session.scope.launch {
            session.documentLane.withLock {
                // Only the dialog this came from, still open: a second tap on
                // Insert before the core answered the first finds it answered.
                if (session.document !== openDocument || state.value.contentEdit?.inserter !== inserter) return@withLock
                when (val result = withContext(session.compute) { openDocument.insertTextRun(inserter.pageIndex, text, bounds) }) {
                    // The new run is outlined from the core's own measure of it.
                    is PdfCoreResult.Success -> content.landed(openDocument, inserter.pageIndex, TEXT_INSERTED, ::dismissInserter)
                    is PdfCoreResult.Failure -> {
                        val now = state.value.contentEdit ?: return@withLock
                        if (now.inserter !== inserter) return@withLock
                        state.value = state.value.copy(contentEdit = now.copy(inserter = inserter.copy(text = text, size = size, error = userMessage(result.error))))
                    }
                }
            }
        }
    }

    private fun arm(addition: ContentAddition, prompt: String) {
        val mode = state.value.contentEdit ?: return
        state.value = state.value.copy(
            contentEdit = mode.copy(adding = addition, movingImage = null, editor = null, resizer = null, inserter = null),
            status = prompt,
        )
    }

    /**
     * Places [bytes] with the top-left corner at [point], at the size the
     * core's placement policy gives them. A file the core cannot read as an
     * image is refused by the placement, before anything is queued.
     */
    private fun insertImage(openDocument: PdfDocument, bytes: ByteArray, pageIndex: Int, point: AnnotationPoint) {
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit == null) return@withLock
                val result = withContext(session.compute) {
                    when (val placement = openDocument.stampPlacement(bytes, point)) {
                        is PdfCoreResult.Success -> openDocument.insertImage(pageIndex, bytes, placement.value)
                        is PdfCoreResult.Failure -> placement
                    }
                }
                when (result) {
                    is PdfCoreResult.Success -> content.landed(openDocument, pageIndex, IMAGE_INSERTED)
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }
}
