package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentFontKind
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.ContentTextRun
import kotlin.math.max

/**
 * Edit content mode: a tap on a line of text the page itself paints opens it
 * for retyping, moving or deleting, a tap on an image it paints opens it for
 * resizing, moving, replacing or deleting, and Add text or Add image claims the next tap
 * for something new. [runs] and [images] hold each page's content
 * as last read — only pages that were shown or tapped — so the outlines
 * describe what the page now shows.
 */
data class ContentEditState(
    val runs: Map<Int, List<ContentTextRun>> = emptyMap(),
    val images: Map<Int, List<ContentImage>> = emptyMap(),
    /** The run being retyped, or null while no editor is open. */
    val editor: TextRunEditor? = null,
    /** The image being resized, or null while no resizer is open. Never open with [editor]. */
    val resizer: ImageResizer? = null,
    /**
     * The run or image the next page tap moves, armed from its dialog's Move;
     * null while no move is armed. One field, so only one move is ever armed.
     * Never armed with a dialog open.
     */
    val moving: ContentTarget? = null,
    /**
     * The image a picked file replaces, set once the core recovered its
     * original and the picker is open; null otherwise. Never set with a
     * dialog open.
     */
    val replacingImage: ContentImage? = null,
    /**
     * What the next page tap adds, armed from Add text or Add image; null
     * while nothing is armed. Never armed with [moving] or a dialog open.
     */
    val adding: ContentAddition? = null,
    /** The new line being typed, or null while no insert dialog is open. */
    val inserter: TextInserter? = null,
) {
    /** The run an armed move will place, if what is armed is a run. */
    val movingText: ContentTextRun? get() = (moving as? ContentTarget.Run)?.run

    /** The image an armed move will place, if what is armed is an image. */
    val movingImage: ContentImage? get() = (moving as? ContentTarget.Image)?.image
}

/** What an armed tap adds to the page, its top-left corner on the tap. */
sealed interface ContentAddition {
    /** A new line of text, typed in a dialog the tap opens. */
    data object Text : ContentAddition

    /**
     * The image [bytes] the reader chose, sized by the core. Not a data
     * class: two choices of the same file are two choices, and array
     * equality would compare contents.
     */
    class Image(val bytes: ByteArray) : ContentAddition
}

/**
 * The insert dialog, for a line whose top-left corner is [at] on page
 * [pageIndex]. [text] and [size] start empty and at the default size and,
 * after a refusal, are what the reader typed — kept with the [error], like
 * [TextRunEditor.text].
 */
data class TextInserter(
    val pageIndex: Int,
    val at: AnnotationPoint,
    val text: String = "",
    val size: String = pointsText(DEFAULT_INSERTED_TEXT_SIZE),
    val error: String? = null,
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
internal const val CONTENT_EDIT_ARMED = "Tap text to retype, move or delete it, or an image to resize, move, replace or delete it."
internal const val CONTENT_EDIT_OFF = "Content editing off."
internal const val CONTENT_EDIT_MISSED = "Nothing to edit there. Tap text to retype, move or delete it, or an image to resize, move, replace or delete it."
internal const val CONTENT_EDIT_FORBIDDEN = "This document does not permit content changes."
internal const val TEXT_UPDATED = "Text updated. Save to keep the change."
internal const val TEXT_DELETED = "Text deleted. Save to keep the change."
internal const val TEXT_MOVED = "Text moved. Save to keep the change."
internal const val TEXT_POSITION_UNCHANGED = "Text position unchanged."
internal const val IMAGE_RESIZED = "Image resized. Save to keep the change."
internal const val IMAGE_SIZE_INVALID = "Image dimensions must be finite and greater than zero."
internal const val IMAGE_MOVED = "Image moved. Save to keep the change."
internal const val IMAGE_POSITION_UNCHANGED = "Image position unchanged."
internal const val MOVE_CANCELLED = "Move cancelled."
internal const val IMAGE_DELETED = "Image deleted. Save to keep the change."
internal const val IMAGE_REPLACED = "Image replaced. Save to keep the change."
internal const val IMAGE_REPLACE_CANCELLED = "Replace cancelled."
internal const val IMAGE_UNREADABLE = "The selected image could not be read."
internal const val TEXT_INSERT_PROMPT = "Tap the page where the new text's top-left corner should go."
internal const val IMAGE_INSERT_PROMPT = "Tap the page where the image's top-left corner should go."
internal const val INSERT_CANCELLED = "Insert cancelled."
internal const val TEXT_INSERTED = "Text inserted. Save to keep the change."
internal const val TEXT_INSERT_EMPTY = "Enter a nonempty single line of text."
internal const val TEXT_SIZE_INVALID = "Text size must be between 1 and 72 pt."
internal const val IMAGE_INSERTED = "Image inserted. Save to keep the change."

/** The size a new line starts at, in points; the Windows shell's default. */
internal const val DEFAULT_INSERTED_TEXT_SIZE = 14.0

/** What an armed move asks for; the image stays on its own page. */
internal fun imageMovePrompt(pageIndex: Int) = "Drag the image, or tap page ${pageIndex + 1} where its top-left corner should go."

/** What an armed text move asks for; the run stays on its own page. */
internal fun textMovePrompt(pageIndex: Int) = "Drag the text, or tap page ${pageIndex + 1} where its top-left corner should go."
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
 * Where a tap at [tap] moves a run or an image now at [bounds]: its top-left
 * corner to the tap, its size kept — the corner a form field's move lands on
 * too. Like a resize, not kept on the page: the corner tapped is the corner sent.
 */
internal fun movedRect(bounds: AnnotationRect, tap: AnnotationPoint): AnnotationRect =
    AnnotationRect(tap.x, tap.y - bounds.height, bounds.width, bounds.height)

/**
 * Where a drag from [from] to [to] carries the top-left corner of a box at
 * [bounds]: by the finger's travel, so a box grabbed mid-way keeps its distance
 * from the finger instead of jumping to it. Sent as the tap [movedRect] reads.
 */
internal fun draggedCorner(bounds: AnnotationRect, from: AnnotationPoint, to: AnnotationPoint): AnnotationPoint =
    AnnotationPoint(bounds.x + to.x - from.x, bounds.y + bounds.height + to.y - from.y)

/** Whether a finger down at [point] grabs the box at [bounds]: inside it, or within [reach] points. */
internal fun grabs(bounds: AnnotationRect, point: AnnotationPoint, reach: Double): Boolean = distance(bounds, point) <= reach

/** The box a run or an image now fills. */
internal val ContentTarget.bounds: AnnotationRect
    get() = when (this) {
        is ContentTarget.Run -> run.bounds
        is ContentTarget.Image -> image.bounds
    }

/** The page a run or an image is on. */
internal val ContentTarget.pageIndex: Int
    get() = when (this) {
        is ContentTarget.Run -> run.pageIndex
        is ContentTarget.Image -> image.pageIndex
    }

/**
 * The box a new line of [size] points gets when its top-left corner is at
 * [at], or null for a size outside 1–72 pt, the Windows shell's range. The
 * core reads only its left edge, bottom edge and height — the font size — and
 * measures the width from the text itself, so the width sent is nominal.
 */
internal fun insertedTextRect(at: AnnotationPoint, size: Double): AnnotationRect? {
    if (!size.isFinite() || size < 1.0 || size > 72.0) return null
    return AnnotationRect(at.x, at.y - size, size, size)
}

/** A line the core can insert: not blank, and one line. */
internal fun insertableText(text: String): Boolean = text.isNotBlank() && '\n' !in text && '\r' !in text

private fun distance(rect: AnnotationRect, point: AnnotationPoint): Double {
    val dx = max(max(rect.x - point.x, point.x - (rect.x + rect.width)), 0.0)
    val dy = max(max(rect.y - point.y, point.y - (rect.y + rect.height)), 0.0)
    return kotlin.math.hypot(dx, dy)
}
