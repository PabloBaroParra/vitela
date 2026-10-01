package dev.vitela.pdf.viewer

import androidx.compose.ui.graphics.ImageBitmap
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.withContext

/**
 * Everything the shell keeps by page *position*, re-read when the layout
 * changes. A page edit, and an undo or redo that may have reverted one, moves
 * pages under the reader's feet: the rendered bitmaps, the page sizes, the
 * search hits, the text selection and Organize's document blocks all described
 * the old layout, so they go.
 * Annotations are re-read by the caller — the core reports them at their
 * pages' new positions.
 *
 * An edit that changes how a page looks without moving it needs less: the
 * preview rebuilt and the pages redrawn ([redraw]), everything else kept.
 */
internal class PageLayout(
    private val session: ViewerSession,
    private val reader: ViewerReader,
    private val selection: TextSelecting,
) {
    private val state = session.state

    /** The document whose pages this session has edited, latched like the Windows shell's `_pagesEdited`. */
    private var editedDocumentId = -1L

    /**
     * Whether the open document ever moved, turned or removed a page. From then
     * on any undo or redo may change the layout, so each one re-reads it; before
     * that nothing else does, and the re-read is skipped.
     */
    val edited: Boolean get() = editedDocumentId == state.value.documentId

    fun markEdited() {
        editedDocumentId = state.value.documentId
    }

    /** The document whose page *appearance* this session has edited — a filled field, a retyped run — latched like [edited]. */
    private var redrawnDocumentId = -1L

    /**
     * Whether the open document ever changed how a page looks without moving
     * it. From then on an undo or redo may change a page's pixels, so each one
     * rebuilds the preview ([redraw]); before that nothing else does.
     */
    val redrawn: Boolean get() = redrawnDocumentId == state.value.documentId

    fun markRedrawn() {
        redrawnDocumentId = state.value.documentId
    }

    /**
     * Rebuilds the preview and redraws every page from it, keeping the layout,
     * the search hits and the selections: nothing moved. Called with the
     * document lane held. Returns false, with the reason in the status, when
     * the preview could not be rebuilt.
     */
    suspend fun redraw(document: PdfDocument): Boolean {
        reader.retireRenders()
        val refreshed = withContext(session.compute) { document.refreshPreview() }
        state.value.organize?.let {
            state.value = state.value.copy(organize = it.copy(thumbnails = emptyMap(), version = it.version + 1))
        }
        reader.pagesRedrawn(redrive = state.value.organize == null)
        if (refreshed is PdfCoreResult.Failure) {
            state.value = state.value.copy(status = userMessage(refreshed.error))
            return false
        }
        return true
    }

    /**
     * Rebuilds the preview, then re-reads the page count and sizes. [thumbnails]
     * says how an open grid's pictures travel to the new layout; by default (an
     * undo or redo, which may have done anything) they are all dropped. Called
     * with the document lane held.
     *
     * The preview comes first because rendering reads it, not the live model:
     * without the rebuild every position would still draw the page that sat
     * there when the file was opened. Returns false, with the reason in the
     * status, when the preview could not be rebuilt.
     */
    suspend fun reread(
        document: PdfDocument,
        thumbnails: (Map<Int, ImageBitmap>) -> Map<Int, ImageBitmap> = { emptyMap() },
    ): Boolean {
        // A reader render does not take the lane, and the rebuild closes the
        // preview it may be reading: whatever it brings back is not wanted.
        reader.retireRenders()
        val refreshed = withContext(session.compute) { document.refreshPreview() }
        val (count, sizes) = withContext(session.compute) { document.pageCount to document.pageSizes }
        // Only while the Documents view shows them; it asks again on the way in.
        val blocks = if (state.value.organize?.view == OrganizeView.Documents) {
            (withContext(session.compute) { document.documentBlocks() } as? PdfCoreResult.Success)?.value.orEmpty()
        } else null
        selection.clear()
        val current = state.value
        state.value = current.copy(
            pageCount = count,
            pageSizes = sizes,
            pageIndex = boundedPageIndex(current.pageIndex, count),
            canPrint = count > 0,
            searchHits = emptyList(),
            searchIndex = 0,
            selectedAnnotationId = null,
            textSelection = null,
            organize = current.organize?.let {
                it.copy(thumbnails = thumbnails(it.thumbnails), version = it.version + 1, blocks = blocks ?: it.blocks)
            },
        )
        reader.layoutChanged(redrive = current.organize == null)
        if (refreshed is PdfCoreResult.Failure) {
            state.value = state.value.copy(status = userMessage(refreshed.error))
            return false
        }
        return true
    }
}
