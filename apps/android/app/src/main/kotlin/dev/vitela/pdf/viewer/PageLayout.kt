package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PageEdit
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.withContext

/**
 * Everything the shell keeps by page *position*, re-read when the layout
 * changes. A page edit, and an undo or redo that may have reverted one, moves
 * pages under the reader's feet: the rendered bitmaps, the page sizes, the
 * search hits and the text selection all described the old layout, so they go.
 * Annotations are re-read by the caller — the core reports them at their
 * pages' new positions.
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

    /**
     * Rebuilds the preview, then re-reads the page count and sizes. [edit] says
     * how an open grid's thumbnails travel; without it (an undo or redo) they
     * are all dropped. Called with the document lane held.
     *
     * The preview comes first because rendering reads it, not the live model:
     * without the rebuild every position would still draw the page that sat
     * there when the file was opened. Returns false, with the reason in the
     * status, when the preview could not be rebuilt.
     */
    suspend fun reread(document: PdfDocument, edit: PageEdit? = null): Boolean {
        // A reader render does not take the lane, and the rebuild closes the
        // preview it may be reading: whatever it brings back is not wanted.
        reader.retireRenders()
        val refreshed = withContext(session.compute) { document.refreshPreview() }
        val (count, sizes) = withContext(session.compute) { document.pageCount to document.pageSizes }
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
                it.copy(thumbnails = if (edit == null) emptyMap() else remapAfterEdit(it.thumbnails, edit), version = it.version + 1)
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
