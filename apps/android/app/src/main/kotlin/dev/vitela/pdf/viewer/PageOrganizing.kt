package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.DocumentBlock
import dev.vitela.pdf.core.PageEdit
import dev.vitela.pdf.core.PdfCoreResult
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Organize pages: a grid of thumbnails where a page is moved a step, turned a
 * quarter, or deleted, and a blank page is added — or, in the Documents view,
 * a whole block of pages from one PDF is. Each is one undoable entry in the shared edit log, so
 * Undo and Redo keep working from the reader chrome; the core owns what an
 * edit means and whether the document allows it, this owns only the cards.
 */
internal class PageOrganizing(
    private val session: ViewerSession,
    private val annotations: AnnotationEditing,
    private val layout: PageLayout,
    private val selection: TextSelecting,
) {
    private val state = session.state

    /** Thumbnails being rendered, by position and layout version, so a card is never asked for twice. */
    private val inFlight = mutableSetOf<Pair<Int, Int>>()

    /**
     * Swaps the reader for the grid. An armed tool, a selected annotation, a
     * text selection and the Form fields panel all point at the reader the
     * grid is about to hide.
     */
    fun open() {
        if (session.document == null || state.value.pageCount == 0 || state.value.organize != null) return
        selection.clear()
        inFlight.clear()
        state.value = state.value.copy(
            organize = OrganizeState(),
            formFields = null,
            contentEdit = null,
            selectedAnnotationId = null,
            activeAnnotationTool = AnnotationTool.Pointer,
            status = "Move, turn, delete or add pages. Each change is one undo step.",
        )
    }

    /** Back to the reader, on the page the user was on. */
    fun close() {
        if (state.value.organize == null) return
        inFlight.clear()
        state.value = state.value.copy(organize = null, scrollTarget = boundedPageIndex(state.value.pageIndex, state.value.pageCount))
    }

    fun move(index: Int, delta: Int) {
        neighbourMove(index, delta, state.value.pageCount)?.let(::apply)
    }

    fun rotate(index: Int, delta: Int) = apply(PageEdit.Rotate(index, delta))

    fun delete(index: Int) = apply(PageEdit.Remove(index))

    fun insertBlank(index: Int, landscape: Boolean) = apply(PageEdit.InsertBlank(index, landscape))

    /**
     * Switches between a card per page and a card per document. The blocks are
     * asked of the core on the way into Documents: a Pages-view edit does not
     * re-read them, so whatever the list held may be stale.
     */
    fun show(view: OrganizeView) {
        val openDocument = session.document ?: return
        val organize = state.value.organize ?: return
        if (organize.view == view) return
        state.value = state.value.copy(organize = organize.copy(view = view))
        if (view != OrganizeView.Documents) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val result = withContext(session.compute) { openDocument.documentBlocks() }
                val current = state.value.organize?.takeIf { it.view == OrganizeView.Documents } ?: return@withLock
                state.value = when (result) {
                    is PdfCoreResult.Success -> state.value.copy(organize = current.copy(blocks = result.value))
                    is PdfCoreResult.Failure -> state.value.copy(organize = current.copy(blocks = emptyList()), status = userMessage(result.error))
                }
            }
        }
    }

    fun moveBlock(block: DocumentBlock, delta: Int) = withShownBlock(block) { blocks -> blockMove(blocks, block, delta)?.let(::apply) }

    fun rotateBlock(block: DocumentBlock, delta: Int) = withShownBlock(block) { apply(PageEdit.Rotate(block.start, delta, block.count)) }

    fun deleteBlock(block: DocumentBlock) = withShownBlock(block) { apply(PageEdit.Remove(block.start, block.count)) }

    /**
     * Runs [action] only while [block] is still one the list shows. A card is a
     * snapshot of the last re-read; acting on positions it no longer holds would
     * move, turn or delete pages the user never pointed at.
     */
    private fun withShownBlock(block: DocumentBlock, action: (List<DocumentBlock>) -> Unit) {
        val organize = state.value.organize ?: return
        if (organize.busy) return
        if (block !in organize.blocks) {
            state.value = state.value.copy(status = ORGANIZE_NO_SUCH_PAGE)
            return
        }
        action(organize.blocks)
    }

    private fun apply(edit: PageEdit) {
        val openDocument = session.document ?: return
        val organize = state.value.organize ?: return
        if (organize.busy) return
        organizeRefusal(state.value.pageCount, edit)?.let {
            state.value = state.value.copy(status = it)
            return
        }
        state.value = state.value.copy(organize = organize.copy(busy = true))
        session.scope.launch {
            try {
                session.documentLane.withLock {
                    if (session.document !== openDocument) return@withLock
                    when (val result = withContext(session.compute) { openDocument.applyPageEdit(edit) }) {
                        is PdfCoreResult.Success -> {
                            layout.markEdited()
                            state.value = state.value.copy(isDirty = true, revision = state.value.revision + 1)
                            val shown = layout.reread(openDocument) { remapAfterEdit(it, edit) }
                            // The edit sits in the shared log: Undo must light up, and the
                            // core reports every annotation at its page's new position.
                            annotations.refresh(openDocument)
                            if (shown) state.value = state.value.copy(status = organizeStatus(edit))
                        }
                        is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                    }
                }
            } finally {
                state.value.organize?.let { state.value = state.value.copy(organize = it.copy(busy = false)) }
            }
        }
    }

    /**
     * Renders the thumbnail of the card at [index] when the grid scrolls it into
     * view. Each render takes the document lane on its own, so an edit never
     * interleaves with a render and a thumbnail always describes the layout it
     * was asked of.
     */
    fun requestThumbnail(index: Int) {
        val openDocument = session.document ?: return
        val organize = state.value.organize ?: return
        val version = organize.version
        if (index !in 0 until state.value.pageCount || index in organize.thumbnails || !inFlight.add(index to version)) return
        session.scope.launch {
            try {
                session.documentLane.withLock {
                    if (session.document !== openDocument || state.value.organize?.version != version) return@withLock
                    val dpi = thumbnailDpi(state.value.pageSizes.getOrNull(index), THUMBNAIL_BOX_PX)
                    val bitmap = withContext(session.compute) {
                        (openDocument.renderPage(index, dpi) as? PdfCoreResult.Success)?.value?.toImageBitmap()
                    } ?: return@withLock
                    val current = state.value.organize?.takeIf { it.version == version } ?: return@withLock
                    state.value = state.value.copy(
                        organize = current.copy(thumbnails = trimThumbnails(current.thumbnails + (index to bitmap), index, THUMBNAIL_CACHE_LIMIT)),
                    )
                }
            } finally {
                inFlight.remove(index to version)
            }
        }
    }
}
