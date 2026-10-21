package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Add watermark: the same text or picture stamped across the pages the reader
 * picks, over the page content or behind it.
 *
 * A dialog rather than an armed page tap like Add text, because a watermark
 * has no position to pick — the core centres it on every page it names — and
 * what it does need is a page list, which no single tap can give.
 *
 * The stamp is page content, so it answers to the content-editing permission
 * and the pages show it only from a rebuilt preview ([PageLayout.redraw]).
 * However many pages it covers it is one entry in the shared edit log, so
 * [annotations] refreshes afterwards to light up Undo.
 */
internal class Watermarking(
    private val session: ViewerSession,
    private val annotations: AnnotationEditing,
    private val layout: PageLayout,
) {
    private val state = session.state

    /** Opens the dialog on the core's default looks, or says on the status line why it cannot. */
    fun open() {
        val openDocument = session.document ?: return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                if (!withContext(session.compute) { openDocument.contentEditingAllowed() }) {
                    state.value = state.value.copy(status = CONTENT_EDIT_FORBIDDEN)
                    return@withLock
                }
                when (val defaults = withContext(session.compute) { defaultsOf(openDocument) }) {
                    is PdfCoreResult.Success -> state.value = state.value.copy(
                        watermark = WatermarkEditor(defaults.value.draft(), defaults.value, currentPage = state.value.pageIndex),
                    )
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(defaults.error))
                }
            }
        }
    }

    fun edit(draft: WatermarkDraft) {
        val editor = state.value.watermark ?: return
        state.value = state.value.copy(watermark = editor.copy(draft = draft))
    }

    /**
     * The picture the reader picked, held as read: the core is handed these
     * bytes untouched when the stamp is recorded. A pick landing after the
     * dialog closed is dropped.
     */
    fun choosePicture(bytes: ByteArray) {
        val editor = state.value.watermark ?: return
        state.value = state.value.copy(watermark = editor.copy(draft = editor.draft.copy(picture = WatermarkPicture(bytes)), message = null))
    }

    /** The picked file could not be read; said under the fields, the earlier picture kept. */
    fun pictureUnreadable() {
        val editor = state.value.watermark ?: return
        state.value = state.value.copy(watermark = editor.copy(message = IMAGE_UNREADABLE))
    }

    fun dismiss() {
        if (state.value.watermark != null) state.value = state.value.copy(watermark = null)
    }

    /**
     * Records one stamp over the draft's pages. A refusal — a bad range, blank
     * text, a character the font cannot show, a file that is not a PNG or
     * JPEG — keeps the dialog open with the
     * reason under the fields, so nothing is found out after it closed.
     */
    fun apply() {
        val openDocument = session.document ?: return
        val editor = state.value.watermark ?: return
        session.scope.launch {
            session.documentLane.withLock {
                // Only the dialog this came from, unchanged: a second tap on
                // Add before the core answered the first finds it answered.
                if (session.document !== openDocument || state.value.watermark !== editor) return@withLock
                val draft = editor.draft
                val recorded = withContext(session.compute) {
                    when (val pages = planWatermark(openDocument, draft, editor.currentPage)) {
                        is PdfCoreResult.Success -> when (val added = stamp(openDocument, draft, pages.value)) {
                            is PdfCoreResult.Success -> PdfCoreResult.Success(pages.value.size)
                            is PdfCoreResult.Failure -> added
                        }
                        is PdfCoreResult.Failure -> pages
                    }
                }
                when (recorded) {
                    is PdfCoreResult.Success -> {
                        layout.markRedrawn()
                        state.value = state.value.copy(watermark = null, isDirty = true, revision = state.value.revision + 1)
                        annotations.refresh(openDocument)
                        if (layout.redraw(openDocument)) state.value = state.value.copy(status = watermarkSummary(recorded.value))
                    }
                    is PdfCoreResult.Failure -> {
                        val now = state.value.watermark ?: return@withLock
                        state.value = state.value.copy(watermark = now.copy(message = userMessage(recorded.error)))
                    }
                }
            }
        }
    }

    private fun defaultsOf(document: PdfDocument): PdfCoreResult<WatermarkDefaults> {
        val text = when (val look = document.defaultWatermarkLook()) {
            is PdfCoreResult.Success -> look.value
            is PdfCoreResult.Failure -> return look
        }
        return when (val picture = document.defaultPictureWatermarkLook()) {
            is PdfCoreResult.Success -> PdfCoreResult.Success(WatermarkDefaults(text, picture.value))
            is PdfCoreResult.Failure -> picture
        }
    }

    // planWatermark has already refused a picture draft with no picture.
    private fun stamp(document: PdfDocument, draft: WatermarkDraft, pages: List<Int>): PdfCoreResult<Unit> = when (draft.kind) {
        WatermarkKind.Text -> document.addWatermark(pages, draft.text, draft.look)
        WatermarkKind.Picture -> document.addImageWatermark(pages, checkNotNull(draft.picture).bytes, draft.pictureLook)
    }
}
