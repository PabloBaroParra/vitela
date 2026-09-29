package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.PageCharacters
import dev.vitela.pdf.core.TextRect

/**
 * A text selection: the core's per-line rects to paint on [pageIndex], and the
 * characters they cover, for the clipboard.
 */
data class TextSelection(val pageIndex: Int, val rects: List<TextRect>, val text: String)

/**
 * One drag-select, from the long-press that starts it until the finger lifts.
 *
 * The page's [PageCharacters] load off the main thread, and by the time they
 * arrive the finger has usually moved. So the session records *points*, not
 * carets: the anchor is where the long-press landed, the focus is the latest
 * move, and both resolve to carets only once [attach] supplies the characters.
 * A move made during the load is therefore never lost — the next [selection]
 * reads it.
 *
 * Owns the characters it is given: [close] releases them, and characters that
 * arrive after the session closed (the user started another drag, or replaced
 * the document, mid-load) are released at once rather than leaked.
 */
internal class TextSelectionDrag(val pageIndex: Int, private val anchorPoint: AnnotationPoint) : AutoCloseable {
    private var characters: PageCharacters? = null
    private var focusPoint = anchorPoint
    private var closed = false

    val isLoaded: Boolean get() = characters != null

    /** The finger has lifted; the selection is final once the characters are loaded. */
    var isFinished = false
        private set

    fun finish() {
        isFinished = true
    }

    fun attach(loaded: PageCharacters) {
        if (closed) {
            loaded.close()
            return
        }
        characters?.close()
        characters = loaded
    }

    fun extend(point: AnnotationPoint) {
        focusPoint = point
    }

    /**
     * The selection between the anchor and the latest focus, or null when
     * there is nothing to show: the characters have not loaded, the page has
     * no positioned text, or the drag has not left the anchor's caret.
     */
    fun selection(): TextSelection? {
        val page = characters ?: return null
        val anchor = page.caretAt(anchorPoint) ?: return null
        val focus = page.caretAt(focusPoint) ?: return null
        if (anchor == focus) return null
        val rects = page.rectsIn(anchor, focus)
        if (rects.isEmpty()) return null
        return TextSelection(pageIndex, rects, page.textIn(anchor, focus))
    }

    override fun close() {
        closed = true
        characters?.close()
        characters = null
    }
}
