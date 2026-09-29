package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.PdfCoreResult
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Long-press drag-select over one page's characters, resolved by the core. */
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
        val openDocument = session.document ?: return
        val drag = TextSelectionDrag(pageIndex, point)
        selectionDrag?.close()
        selectionDrag = drag
        state.value = state.value.copy(textSelection = null, selectedAnnotationId = null)
        session.scope.launch {
            when (val result = withContext(Dispatchers.Default) { openDocument.pageCharacters(pageIndex) }) {
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
        state.value = state.value.copy(textSelection = drag.selection())
    }
}
