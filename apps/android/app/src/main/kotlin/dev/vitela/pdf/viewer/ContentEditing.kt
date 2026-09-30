package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.PageContent
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Edit content: retyping a line of text the page itself paints, keeping its
 * position and — unless it is a composite font — its font; and resizing an
 * image it paints, keeping its top-left corner. Each is one undoable entry in
 * the shared edit log.
 *
 * Nothing is drawn in place of the words or the picture: only the renderer can
 * paint them, so an edit rebuilds the preview and the page re-renders showing
 * it — the Windows shell's approach. What this shell draws is the outline of
 * each run and image, from the content as the core last reported it.
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

    /** Pages whose content is being read, so a page shown twice in a row is read once. */
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

    /** The reader laid out page [pageIndex]: read its content once, for the outlines. */
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
     * A tap at [point] on page [pageIndex]: opens the run or image it meant,
     * reading the page first if it has not been. [reach] is how far off an
     * item, in points, a tap may land and still mean it.
     */
    fun tap(pageIndex: Int, point: AnnotationPoint, reach: Double) {
        val openDocument = session.document ?: return
        if (state.value.contentEdit == null) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val known = state.value.contentEdit?.let { mode ->
                    mode.runs[pageIndex]?.let { runs -> PageContent(runs, mode.images[pageIndex].orEmpty()) }
                }
                val content = known ?: read(openDocument, pageIndex) ?: return@withLock
                val mode = state.value.contentEdit ?: return@withLock
                state.value = when (val hit = contentAt(content.textRuns, content.images, point, reach)) {
                    null -> state.value.copy(status = CONTENT_EDIT_MISSED)
                    is ContentTarget.Run -> state.value.copy(contentEdit = mode.copy(editor = TextRunEditor(hit.run), resizer = null))
                    is ContentTarget.Image -> state.value.copy(contentEdit = mode.copy(editor = null, resizer = ImageResizer(hit.image)))
                }
            }
        }
    }

    fun dismissEditor() {
        val mode = state.value.contentEdit ?: return
        if (mode.editor != null) state.value = state.value.copy(contentEdit = mode.copy(editor = null))
    }

    fun dismissResizer() {
        val mode = state.value.contentEdit ?: return
        if (mode.resizer != null) state.value = state.value.copy(contentEdit = mode.copy(resizer = null))
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
                    is PdfCoreResult.Success -> {
                        layout.markRedrawn()
                        state.value = state.value.copy(isDirty = true, revision = state.value.revision + 1)
                        dismissResizer()
                        // The resize sits in the shared log: Undo must light up.
                        annotations.refresh(openDocument)
                        val redrawn = layout.redraw(openDocument)
                        // The outline follows the core's word for the box the image now fills.
                        read(openDocument, image.pageIndex)
                        if (redrawn) state.value = state.value.copy(status = IMAGE_RESIZED)
                    }
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
     * Re-reads every page already read, after an undo or redo — neither says
     * which item it changed. Called with the document lane held.
     */
    suspend fun reread(document: PdfDocument) {
        val mode = state.value.contentEdit ?: return
        state.value = state.value.copy(contentEdit = mode.copy(editor = null, resizer = null))
        for (pageIndex in mode.runs.keys) read(document, pageIndex)
    }

    /**
     * Reads page [pageIndex]'s content into the mode and returns it; null when
     * the core refused, the mode closed or another document arrived meanwhile.
     * A refusal is kept as an empty page, so it is not asked again on every
     * scroll, and its reason is left in the status. Called with the document
     * lane held.
     */
    private suspend fun read(document: PdfDocument, pageIndex: Int): PageContent? {
        val result = withContext(session.compute) { document.pageContent(pageIndex) }
        val mode = state.value.contentEdit ?: return null
        if (session.document !== document) return null
        return when (result) {
            is PdfCoreResult.Success -> result.value.also { content ->
                state.value = state.value.copy(
                    contentEdit = mode.copy(runs = mode.runs + (pageIndex to content.textRuns), images = mode.images + (pageIndex to content.images)),
                )
            }
            is PdfCoreResult.Failure -> {
                state.value = state.value.copy(
                    contentEdit = mode.copy(runs = mode.runs + (pageIndex to emptyList()), images = mode.images + (pageIndex to emptyList())),
                    status = userMessage(result.error),
                )
                null
            }
        }
    }
}
