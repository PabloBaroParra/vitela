package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.PageContent
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Edit content: retyping a line of text the page itself paints — a block, the
 * runs the core groups on one line — keeping its position and, unless its first
 * run is a composite font, its font; moving one, keeping its text, font and
 * size; or deleting it;
 * resizing an image it paints, keeping its top-left corner; moving one,
 * keeping its size; replacing its picture, keeping its box; and deleting
 * one; and the same resize, move and delete for a vector path — a table
 * border, a rule, a shaded box. Each is one undoable entry in the shared edit log.
 *
 * Nothing is drawn in place of the words or the picture: only the renderer can
 * paint them, so an edit rebuilds the preview and the page re-renders showing
 * it — the Windows shell's approach. What this shell draws is the outline of
 * each block, image and path, from the content as the core last reported it.
 *
 * Unlike the Windows shell, which writes as the reader types, a retype here is
 * committed from a dialog: one tap, one edit. A second retype of the same block
 * sends the block as the core now reports it — same first run, new text — and the
 * core folds it into the queued edit instead of stacking another; so does moving
 * a block again, or an image.
 *
 * This class owns the mode — arming it, reading pages, routing a tap and
 * landing an accepted edit. The edits themselves live in [TextBlockEditing],
 * [ImageEditing], [PathEditing], [ContentInserting] and, for several items at
 * once, [SelectionEditing].
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

    /** Retyping, moving and deleting text blocks. */
    val text = TextBlockEditing(session, this)

    /** Resizing, moving, replacing and deleting images. */
    val images = ImageEditing(session, this)

    /** Resizing, moving and deleting vector paths. */
    val paths = PathEditing(session, this)

    /** Adding new text and images. */
    val inserts = ContentInserting(session, this)

    /** Gathering several items on a page, then moving or deleting them at once. */
    val selections = SelectionEditing(session, this)

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

    /** Disarms a move of a text block, an image or a path: the next tap opens what it hits. */
    fun cancelMove() {
        val mode = state.value.contentEdit ?: return
        if (mode.moving != null) state.value = state.value.copy(contentEdit = mode.copy(moving = null), status = MOVE_CANCELLED)
    }

    /** The reader laid out page [pageIndex]: read its content once, for the outlines. */
    fun pageShown(pageIndex: Int) {
        val openDocument = session.document ?: return
        val mode = state.value.contentEdit ?: return
        if (pageIndex in mode.blocks || !reading.add(pageIndex)) return
        session.scope.launch {
            try {
                session.documentLane.withLock {
                    if (session.document === openDocument && state.value.contentEdit?.blocks?.containsKey(pageIndex) == false) read(openDocument, pageIndex)
                }
            } finally {
                reading.remove(pageIndex)
            }
        }
    }

    /**
     * A tap at [point] on page [pageIndex]: places an armed insert or move,
     * else opens the block, image or path it meant, reading the page first if it has not been. [reach] is how far off an
     * item, in points, a tap may land and still mean it.
     */
    fun tap(pageIndex: Int, point: AnnotationPoint, reach: Double) {
        val openDocument = session.document ?: return
        val armed = state.value.contentEdit ?: return
        armed.adding?.let { addition -> return inserts.place(openDocument, addition, pageIndex, point) }
        when (val moving = armed.moving) {
            is ContentTarget.Block -> return text.move(openDocument, moving.block, pageIndex, point)
            is ContentTarget.Image -> return images.move(openDocument, moving.image, pageIndex, point)
            is ContentTarget.Path -> return paths.move(openDocument, moving.path, pageIndex, point)
            null -> Unit
        }
        // With a selection a tap adds to it, takes from it or ends it; it opens no dialog.
        armed.selection?.let { return selections.tap(openDocument, it, pageIndex, point, reach) }
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val content = contentOf(openDocument, pageIndex) ?: return@withLock
                val mode = state.value.contentEdit ?: return@withLock
                state.value = when (val hit = contentAt(content.blocks, content.images, point, reach, content.paths)) {
                    null -> state.value.copy(status = CONTENT_EDIT_MISSED)
                    is ContentTarget.Block -> state.value.copy(contentEdit = mode.copy(editor = TextBlockEditor(hit.block), resizer = null, shaper = null))
                    is ContentTarget.Image -> state.value.copy(contentEdit = mode.copy(editor = null, resizer = ImageResizer(hit.image), shaper = null))
                    is ContentTarget.Path -> state.value.copy(contentEdit = mode.copy(editor = null, resizer = null, shaper = PathShaper(hit.path)))
                }
            }
        }
    }

    /**
     * Lands an edit the core accepted on page [pageIndex]: the page re-renders,
     * Undo lights up — the edit sits in the shared log — and the page is read
     * again, so its outlines follow the core's word for what it now paints.
     * [spend] closes the dialog the edit came from, before Undo is refreshed;
     * [settled] runs once the page has been read again, before [status] is
     * shown — once the page has redrawn. Called with the document lane held.
     */
    suspend fun landed(document: PdfDocument, pageIndex: Int, status: String, spend: () -> Unit = {}, settled: () -> Unit = {}) {
        layout.markRedrawn()
        state.value = state.value.copy(isDirty = true, revision = state.value.revision + 1)
        spend()
        annotations.refresh(document)
        val redrawn = layout.redraw(document)
        read(document, pageIndex)
        settled()
        if (redrawn) state.value = state.value.copy(status = status)
    }

    /**
     * Re-reads every page already read, after an undo or redo — neither says
     * which item it changed. Called with the document lane held.
     */
    suspend fun reread(document: PdfDocument) {
        val mode = state.value.contentEdit ?: return
        // An armed move, an open replacement picker or a selection holds the block,
        // image or path as it was; it may no longer be there. An armed insert and
        // its dialog hold only a spot on a page, which survives.
        state.value = state.value.copy(contentEdit = mode.copy(editor = null, resizer = null, shaper = null, moving = null, replacingImage = null, selection = null))
        for (pageIndex in mode.blocks.keys) read(document, pageIndex)
    }

    /**
     * Page [pageIndex]'s content as the mode last read it, reading it first if
     * it has not; null when the core refused or the mode closed. Called with
     * the document lane held.
     */
    suspend fun contentOf(document: PdfDocument, pageIndex: Int): PageContent? {
        val mode = state.value.contentEdit
        val known = mode?.blocks?.get(pageIndex)?.let { blocks -> PageContent(emptyList(), mode.images[pageIndex].orEmpty(), mode.paths[pageIndex].orEmpty(), blocks) }
        return known ?: read(document, pageIndex)
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
                    contentEdit = mode.copy(
                        blocks = mode.blocks + (pageIndex to content.blocks),
                        images = mode.images + (pageIndex to content.images),
                        paths = mode.paths + (pageIndex to content.paths),
                    ),
                )
            }
            is PdfCoreResult.Failure -> {
                state.value = state.value.copy(
                    contentEdit = mode.copy(
                        blocks = mode.blocks + (pageIndex to emptyList()),
                        images = mode.images + (pageIndex to emptyList()),
                        paths = mode.paths + (pageIndex to emptyList()),
                    ),
                    status = userMessage(result.error),
                )
                null
            }
        }
    }
}
