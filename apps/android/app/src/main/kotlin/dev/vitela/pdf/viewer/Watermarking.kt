package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreResult
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Add watermark: the same text stamped across the pages the reader picks.
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

    /** Opens the dialog on the core's default look, or says on the status line why it cannot. */
    fun open() {
        val openDocument = session.document ?: return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                if (!withContext(session.compute) { openDocument.contentEditingAllowed() }) {
                    state.value = state.value.copy(status = CONTENT_EDIT_FORBIDDEN)
                    return@withLock
                }
                when (val look = withContext(session.compute) { openDocument.defaultWatermarkLook() }) {
                    is PdfCoreResult.Success -> state.value = state.value.copy(
                        watermark = WatermarkEditor(WatermarkDraft.from(look.value), currentPage = state.value.pageIndex),
                    )
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(look.error))
                }
            }
        }
    }

    fun edit(draft: WatermarkDraft) {
        val editor = state.value.watermark ?: return
        state.value = state.value.copy(watermark = editor.copy(draft = draft))
    }

    fun dismiss() {
        if (state.value.watermark != null) state.value = state.value.copy(watermark = null)
    }

    /**
     * Records one stamp over the draft's pages. A refusal — a bad range, blank
     * text, a character the font cannot show — keeps the dialog open with the
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
                        is PdfCoreResult.Success -> when (val added = openDocument.addWatermark(pages.value, draft.text, draft.look)) {
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
}
