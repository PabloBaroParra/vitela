package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.PageCharacters
import dev.vitela.pdf.core.TextRect

/**
 * A text selection: the core's per-line rects to paint on [pageIndex], the
 * characters they cover, for the clipboard, and the carets that bound them —
 * [start] never after [end] — which a handle drag anchors to.
 */
data class TextSelection(val pageIndex: Int, val rects: List<TextRect>, val text: String, val start: Int, val end: Int)

/**
 * One drag-select, from the long-press (or handle grab) that starts it until
 * the finger lifts. A long-press that never moves selects the word under it; a
 * handle drag keeps the opposite end's caret and moves only the grabbed one.
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
internal class TextSelectionDrag(val pageIndex: Int, private val anchor: Anchor, private var focusPoint: AnnotationPoint) : AutoCloseable {
    /** A long-press drag: anchored where the press landed. */
    constructor(pageIndex: Int, pressedAt: AnnotationPoint) : this(pageIndex, Anchor.Press(pressedAt), pressedAt)

    /** Where the fixed end of the selection is. */
    sealed interface Anchor {
        /** A long-press: a point, resolved once the characters load. */
        data class Press(val point: AnnotationPoint) : Anchor

        /** A handle drag: the caret of the end that was not grabbed, already known. */
        data class Caret(val caret: Int) : Anchor
    }

    private var characters: PageCharacters? = null
    private var closed = false

    /**
     * A handle drag that collapses onto its anchor selects nothing; the caller
     * should keep the selection it already shows rather than blink it away.
     */
    val keepsShownSelection: Boolean get() = anchor is Anchor.Caret

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
     * there is nothing to show: the characters have not loaded, or the page
     * has no positioned text.
     *
     * While a long-press has not left the anchor's caret, the selection is
     * the word under the anchor instead, as the core bounds it; a press on
     * whitespace still selects nothing. A handle drag has no such fallback.
     */
    fun selection(): TextSelection? {
        val page = characters ?: return null
        var focus = page.caretAt(focusPoint) ?: return null
        var anchorCaret = when (anchor) {
            is Anchor.Caret -> anchor.caret
            is Anchor.Press -> page.caretAt(anchor.point) ?: return null
        }
        if (anchorCaret == focus) {
            if (anchor !is Anchor.Press) return null
            val word = page.wordAt(anchor.point) ?: return null
            anchorCaret = word.start
            focus = word.end
        }
        val rects = page.rectsIn(anchorCaret, focus)
        if (rects.isEmpty()) return null
        return TextSelection(pageIndex, rects, page.textIn(anchorCaret, focus), minOf(anchorCaret, focus), maxOf(anchorCaret, focus))
    }

    override fun close() {
        closed = true
        characters?.close()
        characters = null
    }
}
