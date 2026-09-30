package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentFontKind
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.ContentTextRun
import kotlin.math.max

/**
 * Edit content mode: a tap on a line of text the page itself paints opens it
 * for retyping, a tap on an image it paints opens it for resizing or moving. [runs] and
 * [images] hold each page's content as last read — only pages that were shown
 * or tapped — so the outlines describe what the page now shows.
 */
data class ContentEditState(
    val runs: Map<Int, List<ContentTextRun>> = emptyMap(),
    val images: Map<Int, List<ContentImage>> = emptyMap(),
    /** The run being retyped, or null while no editor is open. */
    val editor: TextRunEditor? = null,
    /** The image being resized, or null while no resizer is open. Never open with [editor]. */
    val resizer: ImageResizer? = null,
    /**
     * The image the next page tap moves, armed from the resize dialog's Move;
     * null while no move is armed. Never armed with a dialog open.
     */
    val movingImage: ContentImage? = null,
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

/**
 * The resize dialog. [width] and [height] start as the image's, in points, and
 * after a refusal are what the reader typed — kept with the [error], like
 * [TextRunEditor.text].
 */
data class ImageResizer(
    val image: ContentImage,
    val width: String = pointsText(image.bounds.width),
    val height: String = pointsText(image.bounds.height),
    val error: String? = null,
)

/** What a tap in the mode landed on. */
sealed interface ContentTarget {
    data class Run(val run: ContentTextRun) : ContentTarget
    data class Image(val image: ContentImage) : ContentTarget
}

// Wording follows the Windows shell's where it has one.
internal const val CONTENT_EDIT_ARMED = "Tap text to retype it, or an image to resize or move it."
internal const val CONTENT_EDIT_OFF = "Content editing off."
internal const val CONTENT_EDIT_MISSED = "Nothing to edit there. Tap text to retype it, or an image to resize or move it."
internal const val CONTENT_EDIT_FORBIDDEN = "This document does not permit content changes."
internal const val TEXT_UPDATED = "Text updated. Save to keep the change."
internal const val IMAGE_RESIZED = "Image resized. Save to keep the change."
internal const val IMAGE_SIZE_INVALID = "Image dimensions must be finite and greater than zero."
internal const val IMAGE_MOVED = "Image moved. Save to keep the change."
internal const val IMAGE_POSITION_UNCHANGED = "Image position unchanged."
internal const val IMAGE_MOVE_CANCELLED = "Move cancelled."

/** What an armed move asks for; the image stays on its own page. */
internal fun imageMovePrompt(pageIndex: Int) = "Tap page ${pageIndex + 1} where the image's top-left corner should go."
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

/**
 * What a tap at [point] meant. Text wins where the finger landed on it — a
 * caption printed over a photo — then an image the tap is inside, then the
 * nearest text within [reach], then an image within it. An image is a large
 * target and text a thin one, so reach only rescues a tap that hit neither.
 */
internal fun contentAt(runs: List<ContentTextRun>, images: List<ContentImage>, point: AnnotationPoint, reach: Double): ContentTarget? =
    textRunAt(runs, point, 0.0)?.let(ContentTarget::Run)
        ?: imageAt(images, point, 0.0)?.let(ContentTarget::Image)
        ?: textRunAt(runs, point, reach)?.let(ContentTarget::Run)
        ?: imageAt(images, point, reach)?.let(ContentTarget::Image)

private fun imageAt(images: List<ContentImage>, point: AnnotationPoint, reach: Double): ContentImage? =
    images.map { it to distance(it.bounds, point) }
        .filter { (_, distance) -> distance <= reach }
        .minWithOrNull(compareBy<Pair<ContentImage, Double>> { it.second }.thenBy { it.first.bounds.width * it.first.bounds.height })
        ?.first

/**
 * The box an image at [bounds] fills at [width] by [height] points, or null
 * for a size that is not a finite, positive number. Its top-left corner stays
 * put, like a resized form field's. Unlike a field it is not kept on the page:
 * an image may already hang off it, and the size typed is the size sent.
 */
internal fun resizedImageRect(bounds: AnnotationRect, width: Double, height: Double): AnnotationRect? {
    if (!width.isFinite() || !height.isFinite() || width <= 0.0 || height <= 0.0) return null
    return AnnotationRect(bounds.x, bounds.y + bounds.height - height, width, height)
}

/**
 * Where a tap at [tap] moves an image now at [bounds]: its top-left corner to
 * the tap, its size kept — the corner a form field's move lands on too.
 * Like a resize, not kept on the page: the corner tapped is the corner sent.
 */
internal fun movedImageRect(bounds: AnnotationRect, tap: AnnotationPoint): AnnotationRect =
    AnnotationRect(tap.x, tap.y - bounds.height, bounds.width, bounds.height)

private fun distance(rect: AnnotationRect, point: AnnotationPoint): Double {
    val dx = max(max(rect.x - point.x, point.x - (rect.x + rect.width)), 0.0)
    val dy = max(max(rect.y - point.y, point.y - (rect.y + rect.height)), 0.0)
    return kotlin.math.hypot(dx, dy)
}
