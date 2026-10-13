package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.PdfCoreResult
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Long-press drag-select, handle drags and double-tap words over one page's characters, resolved by the core. */
internal class TextSelecting(private val session: ViewerSession) {
    private val state = session.state

    /** The drag-select in progress, or finished but still waiting for its page's characters. */
    private var selectionDrag: TextSelectionDrag? = null

    /**
     * A long-press on [pageIndex] starts a drag-select anchored at [point].
     * The page's characters load off the main thread; moves that arrive
     * before they do are kept by the [TextSelectionDrag], not dropped.
     */
    fun begin(pageIndex: Int, point: AnnotationPoint) {
        if (session.document == null) return
        state.value = state.value.copy(textSelection = null, selectedAnnotationId = null)
        start(TextSelectionDrag(pageIndex, point))
    }

    /**
     * A finger grabbed one of the shown selection's handles at [point]: a new
     * drag that keeps the other end's caret. The selection stays painted while
     * the page's characters reload — they were released when the last drag ended.
     */
    fun grab(handle: SelectionHandle, point: AnnotationPoint) {
        val shown = state.value.textSelection ?: return
        val anchor = if (handle == SelectionHandle.Start) shown.end else shown.start
        start(TextSelectionDrag(shown.pageIndex, TextSelectionDrag.Anchor.Caret(anchor), point))
    }

    /** A double tap: the word under [point], exactly as a long-press that never moved. */
    fun selectWord(pageIndex: Int, point: AnnotationPoint) {
        begin(pageIndex, point)
        end()
    }

    private fun start(drag: TextSelectionDrag) {
        val openDocument = session.document ?: return
        selectionDrag?.close()
        selectionDrag = drag
        session.scope.launch {
            when (val result = withContext(session.compute) { openDocument.pageCharacters(drag.pageIndex) }) {
                is PdfCoreResult.Success -> {
                    // A closed drag releases late characters itself.
                    drag.attach(result.value)
                    if (selectionDrag === drag) settle(drag)
                }
                is PdfCoreResult.Failure -> if (selectionDrag === drag) {
                    selectionDrag = null
                    drag.close()
                    state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }

    /**
     * The finger moved during a drag-select. Resolved on the calling thread:
     * a caret lookup is a scan of one page's characters in Rust, far cheaper
     * than a coroutine hop per pointer event.
     */
    fun extend(point: AnnotationPoint) {
        val drag = selectionDrag ?: return
        drag.extend(point)
        if (drag.isLoaded) publish(drag)
    }

    /** The finger lifted: the selection stays, and the page's characters are released. */
    fun end() {
        val drag = selectionDrag ?: return
        drag.finish()
        if (drag.isLoaded) settle(drag)
    }

    fun clear() {
        closeDrag()
        if (state.value.textSelection != null) state.value = state.value.copy(textSelection = null)
    }

    /** Releases the drag's characters without touching the published selection. */
    fun closeDrag() {
        selectionDrag?.close()
        selectionDrag = null
    }

    private fun settle(drag: TextSelectionDrag) {
        publish(drag)
        if (drag.isFinished) closeDrag()
    }

    private fun publish(drag: TextSelectionDrag) {
        val selection = drag.selection()
        if (selection == null && drag.keepsShownSelection) return
        state.value = state.value.copy(textSelection = selection)
    }
}
