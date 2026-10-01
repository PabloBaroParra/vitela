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
 * position and — unless it is a composite font — its font; moving one,
 * keeping its text, font and size; or deleting it;
 * resizing an image it paints, keeping its top-left corner; moving one,
 * keeping its size; replacing its picture, keeping its box; and deleting
 * one. Each is one undoable entry in the shared edit log.
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
 *
 * This class owns the mode — arming it, reading pages, routing a tap and
 * landing an accepted edit. The edits themselves live in [TextRunEditing],
 * [ImageEditing] and [ContentInserting].
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

    /** Retyping, moving and deleting text runs. */
    val text = TextRunEditing(session, this)

    /** Resizing, moving, replacing and deleting images. */
    val images = ImageEditing(session, this)

    /** Adding new text and images. */
    val inserts = ContentInserting(session, this)

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

    /** Disarms a move of a run or an image: the next tap opens what it hits. */
    fun cancelMove() {
        val mode = state.value.contentEdit ?: return
        if (mode.moving != null) state.value = state.value.copy(contentEdit = mode.copy(moving = null), status = MOVE_CANCELLED)
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
     * A tap at [point] on page [pageIndex]: places an armed insert or move,
     * else opens the run or image it meant, reading the page first if it has not been. [reach] is how far off an
     * item, in points, a tap may land and still mean it.
     */
    fun tap(pageIndex: Int, point: AnnotationPoint, reach: Double) {
        val openDocument = session.document ?: return
        val armed = state.value.contentEdit ?: return
        armed.adding?.let { addition -> return inserts.place(openDocument, addition, pageIndex, point) }
        when (val moving = armed.moving) {
            is ContentTarget.Run -> return text.move(openDocument, moving.run, pageIndex, point)
            is ContentTarget.Image -> return images.move(openDocument, moving.image, pageIndex, point)
            null -> Unit
        }
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

    /**
     * Lands an edit the core accepted on page [pageIndex]: the page re-renders,
     * Undo lights up — the edit sits in the shared log — and the page is read
     * again, so its outlines follow the core's word for what it now paints.
     * [spend] closes the dialog the edit came from, before Undo is refreshed;
     * [status] is shown once the page has redrawn. Called with the document
     * lane held.
     */
    suspend fun landed(document: PdfDocument, pageIndex: Int, status: String, spend: () -> Unit = {}) {
        layout.markRedrawn()
        state.value = state.value.copy(isDirty = true, revision = state.value.revision + 1)
        spend()
        annotations.refresh(document)
        val redrawn = layout.redraw(document)
        read(document, pageIndex)
        if (redrawn) state.value = state.value.copy(status = status)
    }

    /**
     * Re-reads every page already read, after an undo or redo — neither says
     * which item it changed. Called with the document lane held.
     */
    suspend fun reread(document: PdfDocument) {
        val mode = state.value.contentEdit ?: return
        // An armed move or an open replacement picker holds the run or image
        // as it was; it may no longer be there. An armed insert and its dialog
        // hold only a spot on a page, which survives.
        state.value = state.value.copy(contentEdit = mode.copy(editor = null, resizer = null, moving = null, replacingImage = null))
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
