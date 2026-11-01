package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentFontKind
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.ContentPath
import dev.vitela.pdf.core.ContentSelection
import dev.vitela.pdf.core.ContentTextBlock
import kotlin.math.max

/**
 * Edit content mode: a tap on a line of text the page itself paints opens the
 * whole line — a block of runs — for retyping, moving or deleting, a tap on an image it paints opens it for
 * resizing, moving, replacing or deleting, a tap on a vector path — a table
 * border, a rule, a shaded box — opens it for resizing, moving or deleting, and
 * Add text or Add image claims the next tap for something new. [blocks],
 * [images] and [paths] hold each page's content as last read — only pages that were shown or tapped — so the outlines
 * describe what the page now shows. A long-press, or a long-press and drag, gathers
 * several items into a [selection] instead.
 */
data class ContentEditState(
    val blocks: Map<Int, List<ContentTextBlock>> = emptyMap(),
    val images: Map<Int, List<ContentImage>> = emptyMap(),
    val paths: Map<Int, List<ContentPath>> = emptyMap(),
    /** The text block being retyped, or null while no editor is open. */
    val editor: TextBlockEditor? = null,
    /** The image being resized, or null while no resizer is open. Never open with [editor]. */
    val resizer: ImageResizer? = null,
    /** The vector path being reshaped, or null while no shape dialog is open. Never open with [editor] or [resizer]. */
    val shaper: PathShaper? = null,
    /**
     * The text block, image or path the next page tap moves, armed from its dialog's Move;
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
    /**
     * Several items on one page, gathered by a marquee or a long-press and kept
     * by taps (B25 T-208), moved or deleted as one undo step; null while
     * nothing is selected. Members are held as last read. Never set with a
     * dialog open, [moving] or [adding]: every place that opens one drops it.
     */
    val selection: ContentSelection? = null,
) {
    /** Whether nothing is open or armed, so a selection may start. */
    val idle: Boolean get() = editor == null && resizer == null && shaper == null && inserter == null &&
        moving == null && replacingImage == null && adding == null

    /** The text block an armed move will place, if what is armed is a block. */
    val movingBlock: ContentTextBlock? get() = (moving as? ContentTarget.Block)?.block

    /** The image an armed move will place, if what is armed is an image. */
    val movingImage: ContentImage? get() = (moving as? ContentTarget.Image)?.image

    /** The path an armed move will place, if what is armed is a path. */
    val movingPath: ContentPath? get() = (moving as? ContentTarget.Path)?.path
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
 * [TextBlockEditor.text].
 */
data class TextInserter(
    val pageIndex: Int,
    val at: AnnotationPoint,
    val text: String = "",
    val size: String = pointsText(DEFAULT_INSERTED_TEXT_SIZE),
    val error: String? = null,
)

/**
 * The retype dialog. [text] starts as the block's and, after a refusal, is what
 * the reader typed — kept with the [error], so the one failure they can fix
 * (a character the font cannot show) is fixed by typing something else.
 */
data class TextBlockEditor(
    val block: ContentTextBlock,
    val text: String = block.text,
    val error: String? = null,
)

/**
 * The resize dialog. [width] and [height] start as the image's, in points, and
 * after a refusal are what the reader typed — kept with the [error], like
 * [TextBlockEditor.text].
 */
data class ImageResizer(
    val image: ContentImage,
    val width: String = pointsText(image.bounds.width),
    val height: String = pointsText(image.bounds.height),
    val error: String? = null,
)

/** What a tap in the mode landed on. */
sealed interface ContentTarget {
    data class Block(val block: ContentTextBlock) : ContentTarget
    data class Image(val image: ContentImage) : ContentTarget
    data class Path(val path: ContentPath) : ContentTarget
}

// Wording follows the Windows shell's where it has one.
internal const val CONTENT_EDIT_ARMED = "Tap a line of text to retype, move or delete it, an image to resize, move, replace or delete it, or a line or box to resize, move or delete it."
internal const val CONTENT_EDIT_OFF = "Content editing off."
internal const val CONTENT_EDIT_MISSED = "Nothing to edit there. Tap a line of text to retype, move or delete it, an image to resize, move, replace or delete it, or a line or box to resize, move or delete it."
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

/** What an armed text move asks for; the block stays on its own page. */
internal fun textMovePrompt(pageIndex: Int) = "Drag the text, or tap page ${pageIndex + 1} where its top-left corner should go."
internal const val FONT_SUBSTITUTED = "This text's font cannot be kept. What you type will use a standard font."

/**
 * The core cannot re-encode a composite (CID) font, so a retype of a block whose
 * first run is one — the run that takes all the text — swaps in a standard font.
 * Worth saying before the reader types: nothing on the page tells that block
 * apart from any other.
 */
internal val ContentTextBlock.substitutesFont: Boolean get() = runs.first().fontKind == ContentFontKind.EmbeddedComposite

/**
 * The block a tap at [point] meant: one it lands inside — on any of its runs or
 * the word gap between them — else the nearest within [reach] points, a finger
 * being wider than a line of small print. Among blocks equally near (a block
 * drawn over a wider one), the smallest wins: it is the one whose edges the
 * reader can see.
 */
internal fun textBlockAt(blocks: List<ContentTextBlock>, point: AnnotationPoint, reach: Double): ContentTextBlock? =
    blocks.map { it to distance(it.bounds, point) }
        .filter { (_, distance) -> distance <= reach }
        .minWithOrNull(compareBy<Pair<ContentTextBlock, Double>> { it.second }.thenBy { it.first.bounds.width * it.first.bounds.height })
        ?.first

/**
 * What a tap at [point] meant. Text wins where the finger landed on it — a
 * caption printed over a photo — then an image the tap is inside, then a
 * path it is on, then the nearest text within [reach], then an image within
 * it, then a path. An image is a large target and text a thin one, and a
 * path is under both — the frame around a block of text — so reach only
 * rescues a tap that hit none of them.
 */
internal fun contentAt(
    blocks: List<ContentTextBlock>,
    images: List<ContentImage>,
    point: AnnotationPoint,
    reach: Double,
    paths: List<ContentPath> = emptyList(),
): ContentTarget? =
    textBlockAt(blocks, point, 0.0)?.let(ContentTarget::Block)
        ?: imageAt(images, point, 0.0)?.let(ContentTarget::Image)
        ?: pathAt(paths, point, 0.0)?.let(ContentTarget::Path)
        ?: textBlockAt(blocks, point, reach)?.let(ContentTarget::Block)
        ?: imageAt(images, point, reach)?.let(ContentTarget::Image)
        ?: pathAt(paths, point, reach)?.let(ContentTarget::Path)

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
 * Where a tap at [tap] moves a text block, an image or a path now at [bounds]: its top-left
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

/** The box a text block, an image or a path now fills; a path's is its geometry, not padded: see [drawnBounds]. */
internal val ContentTarget.bounds: AnnotationRect
    get() = when (this) {
        is ContentTarget.Block -> block.bounds
        is ContentTarget.Image -> image.bounds
        is ContentTarget.Path -> path.bounds
    }

/** The page a text block, an image or a path is on. */
internal val ContentTarget.pageIndex: Int
    get() = when (this) {
        is ContentTarget.Block -> block.pageIndex
        is ContentTarget.Image -> image.pageIndex
        is ContentTarget.Path -> path.pageIndex
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
