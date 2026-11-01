package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentSelection
import dev.vitela.pdf.core.PageContent
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * The several-at-once half of Edit content (B25 T-208): gathering text blocks,
 * images and vector paths on one page into a selection, then moving or deleting
 * all of it as one undo step. A touch screen has no SHIFT and no mouse rubber
 * band, so the desktop gestures become these:
 *
 * - a long-press and drag is the marquee, and everything whose box touches it is
 *   selected ([marquee]); a long-press that does not move selects the item under
 *   it ([press]). Either one starts a new selection, replacing any there was;
 * - with a selection, a tap on an item of the same page adds it or — if already
 *   in — takes it out, which is SHIFT+click; a tap on empty page, or on another
 *   page, ends the selection and does nothing else ([tap]);
 * - a drag that starts inside the selection's dashed box moves all of it
 *   ([move]), and [delete] removes it.
 *
 * A selection never coexists with a dialog, an armed move or an armed insert:
 * it cannot start while one is open, and each of them drops it. An undo or redo
 * drops it too, since its members may not be what the page now paints. After a
 * move the members are looked up again by id in the fresh read, so the selection
 * stays on what moved, as the Windows shell's does; after a delete it ends.
 *
 * [content] owns the mode and lands each accepted edit.
 */
internal class SelectionEditing(
    private val session: ViewerSession,
    private val content: ContentEditing,
) {
    private val state = session.state

    /**
     * Selects everything on page [pageIndex] whose box touches [area], in PDF
     * points; nothing at all leaves no selection and says so.
     */
    fun marquee(pageIndex: Int, area: AnnotationRect) = gather(pageIndex) { page ->
        selectionIn(pageIndex, page.blocks, page.images, page.paths, area)
    }

    /**
     * Selects the item a long-press at [point] on page [pageIndex] lands on,
     * found the way a tap finds it, [reach] points of slack included.
     */
    fun press(pageIndex: Int, point: AnnotationPoint, reach: Double) = gather(pageIndex) { page ->
        when (val hit = contentAt(page.blocks, page.images, point, reach, page.paths)) {
            null -> null
            else -> ContentSelection(pageIndex).toggled(hit)
        }
    }

    /**
     * A tap at [point] on page [pageIndex] while [selection] exists: adds the
     * item it hits, takes it out if it is a member, or — on empty page or
     * another page — ends the selection. Nothing opens.
     */
    fun tap(openDocument: PdfDocument, selection: ContentSelection, pageIndex: Int, point: AnnotationPoint, reach: Double) {
        if (pageIndex != selection.pageIndex) {
            set(null, SELECTION_CLEARED)
            return
        }
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val page = content.contentOf(openDocument, pageIndex) ?: return@withLock
                val current = state.value.contentEdit?.selection ?: return@withLock
                val next = contentAt(page.blocks, page.images, point, reach, page.paths)?.let(current::toggled)
                set(next, SELECTION_CLEARED)
            }
        }
    }

    /**
     * Moves the selection by ([dx], [dy]) points — the corner it was dragged to
     * minus the one it was grabbed at — as one undo step. A drag on another page
     * than the selection's moves nothing, and one that went nowhere queues
     * nothing. A refusal is reported in the status line and the selection kept.
     */
    fun move(pageIndex: Int, dx: Double, dy: Double) {
        val openDocument = session.document ?: return
        val selection = state.value.contentEdit?.selection?.takeIf { it.pageIndex == pageIndex } ?: return
        if (!dx.isFinite() || !dy.isFinite()) return
        if (dx == 0.0 && dy == 0.0) {
            state.value = state.value.copy(status = SELECTION_POSITION_UNCHANGED)
            return
        }
        session.scope.launch {
            session.documentLane.withLock {
                // Only the selection this drag began on: one already moved, or ended by an undo, is not moved twice.
                if (session.document !== openDocument || state.value.contentEdit?.selection != selection) return@withLock
                when (val result = withContext(session.compute) { openDocument.moveContentSelection(selection, dx, dy) }) {
                    is PdfCoreResult.Success -> content.landed(openDocument, pageIndex, SELECTION_MOVED, settled = { followMoved(selection) })
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }

    /**
     * Deletes the selection as one undo step. Spent before the core answers, like
     * an image's delete: a second tap on Delete finds nothing selected. A refusal
     * is reported in the status line and the selection given back.
     */
    fun delete() {
        val openDocument = session.document ?: return
        val mode = state.value.contentEdit ?: return
        val selection = mode.selection ?: return
        state.value = state.value.copy(contentEdit = mode.copy(selection = null))
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit == null) return@withLock
                when (val result = withContext(session.compute) { openDocument.removeContentSelection(selection) }) {
                    // The core no longer reports the members, so their outlines go too.
                    is PdfCoreResult.Success -> content.landed(openDocument, selection.pageIndex, SELECTION_DELETED)
                    is PdfCoreResult.Failure -> {
                        val now = state.value.contentEdit ?: return@withLock
                        val back = if (now.selection == null && now.idle) selection else now.selection
                        state.value = state.value.copy(contentEdit = now.copy(selection = back), status = userMessage(result.error))
                    }
                }
            }
        }
    }

    /** Ends the selection, touching nothing in the document. */
    fun clear() {
        if (state.value.contentEdit?.selection != null) set(null, SELECTION_CLEARED)
    }

    /**
     * Reads page [pageIndex] if need be and starts a selection from what [pick]
     * finds on it — only while nothing is open or armed. [pick] finding nothing
     * leaves no selection, the old one included, and says so.
     */
    private fun gather(pageIndex: Int, pick: (PageContent) -> ContentSelection?) {
        val openDocument = session.document ?: return
        if (state.value.contentEdit?.idle != true) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val page = content.contentOf(openDocument, pageIndex) ?: return@withLock
                if (state.value.contentEdit?.idle != true) return@withLock
                set(pick(page), SELECTION_EMPTY)
            }
        }
    }

    /** Makes [selection] the selection, saying how many it holds, or [whenNone] if it is null. */
    private fun set(selection: ContentSelection?, whenNone: String) {
        val mode = state.value.contentEdit ?: return
        state.value = state.value.copy(
            contentEdit = mode.copy(selection = selection),
            status = if (selection == null) whenNone else selectedMessage(selection.count),
        )
    }

    /** After a move: [moved] looked up again in the fresh read, so it stays on the members where they landed. */
    private fun followMoved(moved: ContentSelection) {
        val mode = state.value.contentEdit ?: return
        if (mode.selection != moved) return
        val page = moved.pageIndex
        val now = moved.reread(mode.blocks[page].orEmpty(), mode.images[page].orEmpty(), mode.paths[page].orEmpty())
        state.value = state.value.copy(contentEdit = mode.copy(selection = now))
    }
}
