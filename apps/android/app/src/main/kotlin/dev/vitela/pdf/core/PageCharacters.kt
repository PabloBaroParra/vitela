package dev.vitela.pdf.core

/**
 * One page's characters, flattened by the core for caret hit-testing — the
 * Kotlin face of `pdf-ffi`'s `FfiPageCharacters`.
 *
 * Every answer comes from `pdf_render::selection`: which line a point falls
 * on, where the caret lands in it, the one-rect-per-line union to paint, and
 * the text for the clipboard. The shell must not recompute any of it — T-086
 * exists because an earlier Android build did, with a rectangle-overlap test
 * that selected a box instead of reading order.
 *
 * Carets sit *between* characters: `0` is before the first one, `len` after
 * the last. [textIn] and [rectsIn] accept the two carets in either order.
 * Loaded once per drag and held until it ends; [close] frees the native side.
 */
interface PageCharacters : AutoCloseable {
    /** The caret nearest a PDF-space point (bottom-left origin), or null on a page with no positioned text. */
    fun caretAt(point: AnnotationPoint): Int?

    /** The text between two carets, for the clipboard. */
    fun textIn(anchor: Int, focus: Int): String

    /** What to paint between two carets: one rect per visual line, in PDF points. */
    fun rectsIn(anchor: Int, focus: Int): List<TextRect>
}
