package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentFontKind
import dev.vitela.pdf.core.ContentTextRun
import kotlin.math.max

/**
 * Edit text mode: a tap on a line of text the page itself paints opens it for
 * retyping. [runs] holds each page's runs as last read — only pages that were
 * shown or tapped — so the outlines describe what the page now says.
 */
data class ContentEditState(
    val runs: Map<Int, List<ContentTextRun>> = emptyMap(),
    /** The run being retyped, or null while no editor is open. */
    val editor: TextRunEditor? = null,
)

/**
 * The retype dialog. [text] starts as the run's and, after a refusal, is what
 * the reader typed — kept with the [error], so the one failure they can fix
 * (a character the font cannot show) is fixed by typing something else.
 */
data class TextRunEditor(
    val run: ContentTextRun,
    val text: String = run.text,
    val error: String? = null,
)

// Wording follows the Windows shell's where it has one.
internal const val CONTENT_EDIT_ARMED = "Tap a line of text to retype it."
internal const val CONTENT_EDIT_OFF = "Text editing off."
internal const val CONTENT_EDIT_MISSED = "No text there. Tap a line of text to retype it."
internal const val CONTENT_EDIT_FORBIDDEN = "This document does not permit content changes."
internal const val TEXT_UPDATED = "Text updated. Save to keep the change."
internal const val FONT_SUBSTITUTED = "This text's font cannot be kept. What you type will use a standard font."

/**
 * The core cannot re-encode a composite (CID) font, so a retype of such a run
 * swaps in a standard font. Worth saying before the reader types: nothing on
 * the page tells that run apart from any other.
 */
internal val ContentTextRun.substitutesFont: Boolean get() = fontKind == ContentFontKind.EmbeddedComposite

/**
 * The run a tap at [point] meant: one it lands inside, else the nearest within
 * [reach] points — a finger is wider than a line of small print. Among runs
 * equally near (a run drawn over a wider one), the smallest wins: it is the
 * one whose edges the reader can see.
 */
internal fun textRunAt(runs: List<ContentTextRun>, point: AnnotationPoint, reach: Double): ContentTextRun? =
    runs.map { it to distance(it.bounds, point) }
        .filter { (_, distance) -> distance <= reach }
        .minWithOrNull(compareBy<Pair<ContentTextRun, Double>> { it.second }.thenBy { it.first.bounds.width * it.first.bounds.height })
        ?.first

private fun distance(rect: AnnotationRect, point: AnnotationPoint): Double {
    val dx = max(max(rect.x - point.x, point.x - (rect.x + rect.width)), 0.0)
    val dy = max(max(rect.y - point.y, point.y - (rect.y + rect.height)), 0.0)
    return kotlin.math.hypot(dx, dy)
}
