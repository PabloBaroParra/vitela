package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.ContentTextRun
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Edit text: retyping a line of text the page itself paints, keeping its
 * position and — unless it is a composite font — its font. Each retype is one
 * undoable entry in the shared edit log.
 *
 * Nothing is drawn in place of the words: only the renderer can paint them,
 * so a retype rebuilds the preview and the page re-renders showing it — the
 * Windows shell's approach. What this shell draws is the outline of each run,
 * from the runs as the core last reported them.
 *
 * Unlike the Windows shell, which writes as the reader types, a retype here is
 * committed from a dialog: one tap, one edit. A second retype of the same run
 * sends the run as the core now reports it — same id, new text — and the core
 * amends the queued command instead of stacking another.
 */
internal class ContentEditing(
    private val session: ViewerSession,
    private val annotations: AnnotationEditing,
    private val layout: PageLayout,
    private val selection: TextSelecting,
) {
    private val state = session.state

    /** Pages whose runs are being read, so a page shown twice in a row is read once. */
    private val reading = mutableSetOf<Int>()

    /**
     * Arms the mode. Not over the Organize grid, which hides the pages a
     * retype would redraw. One mode claims a page tap at a time, so an armed
     * annotation tool and a text selection are dropped.
     *
     * Armed before the permission is known, then backed out if the core
     * refuses: asked on the UI thread, the question would wait for the
     * document's lock behind whatever render holds it.
     */
    fun open() {
        val openDocument = session.document ?: return
        if (state.value.pageCount == 0 || state.value.organize != null || state.value.contentEdit != null) return
        selection.clear()
        state.value = state.value.copy(
            contentEdit = ContentEditState(),
            activeAnnotationTool = AnnotationTool.Pointer,
            selectedAnnotationId = null,
            status = CONTENT_EDIT_ARMED,
        )
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit == null) return@withLock
                if (!withContext(session.compute) { openDocument.contentEditingAllowed() }) {
                    state.value = state.value.copy(contentEdit = null, status = CONTENT_EDIT_FORBIDDEN)
                }
            }
        }
    }

    fun close() {
        if (state.value.contentEdit != null) state.value = state.value.copy(contentEdit = null, status = CONTENT_EDIT_OFF)
    }

    /** The reader laid out page [pageIndex]: read its runs once, for the outlines. */
    fun pageShown(pageIndex: Int) {
        val openDocument = session.document ?: return
        val mode = state.value.contentEdit ?: return
        if (pageIndex in mode.runs || !reading.add(pageIndex)) return
        session.scope.launch {
            try {
                session.documentLane.withLock {
                    if (session.document === openDocument && state.value.contentEdit?.runs?.containsKey(pageIndex) == false) read(openDocument, pageIndex)
                }
            } finally {
                reading.remove(pageIndex)
            }
        }
    }

    /**
     * A tap at [point] on page [pageIndex]: opens the run it meant, reading the
     * page first if it has not been. [reach] is how far off a run, in points,
     * a tap may land and still mean it.
     */
    fun tap(pageIndex: Int, point: AnnotationPoint, reach: Double) {
        val openDocument = session.document ?: return
        if (state.value.contentEdit == null) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val runs = state.value.contentEdit?.runs?.get(pageIndex) ?: read(openDocument, pageIndex) ?: return@withLock
                val mode = state.value.contentEdit ?: return@withLock
                val hit = textRunAt(runs, point, reach)
                state.value = if (hit == null) {
                    state.value.copy(status = CONTENT_EDIT_MISSED)
                } else {
                    state.value.copy(contentEdit = mode.copy(editor = TextRunEditor(hit)))
                }
            }
        }
    }

    fun dismissEditor() {
        val mode = state.value.contentEdit ?: return
        if (mode.editor != null) state.value = state.value.copy(contentEdit = mode.copy(editor = null))
    }

    /**
     * Retypes the open editor's run as [text], for the document [documentId]
     * the dialog was built for: a dialog confirmed after another document
     * replaced it holds a run that means nothing against the new file.
     */
    fun retype(documentId: Long, text: String) {
        val openDocument = session.document ?: return
        if (documentId != state.value.documentId) return
        val editor = state.value.contentEdit?.editor ?: return
        if (text == editor.run.text) {
            dismissEditor()
            return
        }
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                when (val result = withContext(session.compute) { openDocument.retypeTextRun(editor.run, text) }) {
                    is PdfCoreResult.Success -> {
                        layout.markRedrawn()
                        state.value = state.value.copy(isDirty = true, revision = state.value.revision + 1)
                        dismissEditor()
                        // The retype sits in the shared log: Undo must light up.
                        annotations.refresh(openDocument)
                        val redrawn = layout.redraw(openDocument)
                        // The run is wider or narrower now; the outline follows the core's word for it.
                        read(openDocument, editor.run.pageIndex)
                        if (redrawn) state.value = state.value.copy(status = TEXT_UPDATED)
                    }
                    is PdfCoreResult.Failure -> {
                        val mode = state.value.contentEdit ?: return@withLock
                        if (mode.editor?.run != editor.run) return@withLock
                        state.value = state.value.copy(contentEdit = mode.copy(editor = editor.copy(text = text, error = userMessage(result.error))))
                    }
                }
            }
        }
    }

    /**
     * Re-reads every page already read, after an undo or redo — neither says
     * which run it moved. Called with the document lane held.
     */
    suspend fun reread(document: PdfDocument) {
        val mode = state.value.contentEdit ?: return
        state.value = state.value.copy(contentEdit = mode.copy(editor = null))
        for (pageIndex in mode.runs.keys) read(document, pageIndex)
    }

    /**
     * Reads page [pageIndex]'s runs into the mode and returns them; null when
     * the core refused, the mode closed or another document arrived meanwhile.
     * A refusal is kept as no runs, so the page is not asked again on every
     * scroll, and its reason is left in the status. Called with the document
     * lane held.
     */
    private suspend fun read(document: PdfDocument, pageIndex: Int): List<ContentTextRun>? {
        val result = withContext(session.compute) { document.pageTextRuns(pageIndex) }
        val mode = state.value.contentEdit ?: return null
        if (session.document !== document) return null
        return when (result) {
            is PdfCoreResult.Success -> result.value.also { runs ->
                state.value = state.value.copy(contentEdit = mode.copy(runs = mode.runs + (pageIndex to runs)))
            }
            is PdfCoreResult.Failure -> {
                state.value = state.value.copy(contentEdit = mode.copy(runs = mode.runs + (pageIndex to emptyList())), status = userMessage(result.error))
                null
            }
        }
    }
}
