package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.ContentPath
import dev.vitela.pdf.core.ContentSelection
import dev.vitela.pdf.core.ContentTextBlock
import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min

// Wording follows the Windows and Linux shells'; a touch screen has no SHIFT or Esc, so the hint names taps instead.
private const val SELECTION_HINT = "drag inside the dashed box to move them, Delete removes them, tap an item to add or remove it."
internal const val SELECTION_EMPTY = "Nothing there to select."
internal const val SELECTION_CLEARED = "Selection cleared."
internal const val SELECTION_MOVED = "Selection moved. Save to keep the change."
internal const val SELECTION_DELETED = "Selection deleted. Save to keep the change."
internal const val SELECTION_POSITION_UNCHANGED = "Selection position unchanged."

/** What the status line says once [count] items are selected. */
internal fun selectedMessage(count: Int) = "$count ${if (count == 1) "item" else "items"} selected — $SELECTION_HINT"

/** The box a marquee dragged from [from] to [to] covers, whichever corner it started at. */
internal fun marqueeRect(from: AnnotationPoint, to: AnnotationPoint) =
    AnnotationRect(min(from.x, to.x), min(from.y, to.y), abs(to.x - from.x), abs(to.y - from.y))

/**
 * Whether [this] and [other] share any point, edges included — so a hairline
 * rule, a box of height zero, is gathered by a marquee that merely crosses it:
 * the Windows rule, decided with the user (B25 T-208).
 */
internal fun AnnotationRect.touches(other: AnnotationRect): Boolean =
    x <= other.x + other.width && other.x <= x + width && y <= other.y + other.height && other.y <= y + height

/**
 * Everything on page [pageIndex] whose box touches [area], or null when none
 * does. Paths are tested by their geometry, not the padded box they are
 * outlined in: the pad is for a finger, a marquee is a deliberate rectangle.
 */
internal fun selectionIn(
    pageIndex: Int,
    blocks: List<ContentTextBlock>,
    images: List<ContentImage>,
    paths: List<ContentPath>,
    area: AnnotationRect,
): ContentSelection? =
    ContentSelection(
        pageIndex,
        blocks.filter { it.bounds.touches(area) },
        images.filter { it.bounds.touches(area) },
        paths.filter { it.bounds.touches(area) },
    ).takeIf { it.count > 0 }

/** Whether [target] is one of the members, by id: a member that has since moved is still one. */
internal operator fun ContentSelection.contains(target: ContentTarget): Boolean = when (target) {
    is ContentTarget.Block -> blocks.any { it.id == target.block.id }
    is ContentTarget.Image -> images.any { it.id == target.image.id }
    is ContentTarget.Path -> paths.any { it.id == target.path.id }
}

/**
 * [this] with [target] added, or taken out if it was a member — the tap that
 * stands in for the SHIFT+click of a desktop. Null once nothing is left.
 */
internal fun ContentSelection.toggled(target: ContentTarget): ContentSelection? {
    val member = target in this
    val next = when (target) {
        is ContentTarget.Block -> copy(blocks = if (member) blocks.filter { it.id != target.block.id } else blocks + target.block)
        is ContentTarget.Image -> copy(images = if (member) images.filter { it.id != target.image.id } else images + target.image)
        is ContentTarget.Path -> copy(paths = if (member) paths.filter { it.id != target.path.id } else paths + target.path)
    }
    return next.takeIf { it.count > 0 }
}

/**
 * The box around every member, each in the box it is drawn in — a path's
 * padded, so a hairline has some height. The dashed rectangle the reader sees
 * and the one a drag must start in.
 */
internal val ContentSelection.bounds: AnnotationRect
    get() {
        val boxes = blocks.map { it.bounds } + images.map { it.bounds } + paths.map { it.bounds.padded(PATH_PAD) }
        if (boxes.isEmpty()) return AnnotationRect(0.0, 0.0, 0.0, 0.0)
        val left = boxes.minOf { it.x }
        val bottom = boxes.minOf { it.y }
        val right = boxes.maxOf { it.x + it.width }
        val top = boxes.maxOf { it.y + it.height }
        return AnnotationRect(left, bottom, right - left, top - bottom)
    }

/** Every member as a target, in the order they are drawn: paths, images, then text. */
internal val ContentSelection.targets: List<ContentTarget>
    get() = paths.map(ContentTarget::Path) + images.map(ContentTarget::Image) + blocks.map(ContentTarget::Block)

/**
 * [this] as the fresh read of its page reports it, matched by id: a moved
 * member is found where it now is, one the read no longer has drops out, and
 * null means none is left.
 */
internal fun ContentSelection.reread(blocks: List<ContentTextBlock>, images: List<ContentImage>, paths: List<ContentPath>): ContentSelection? {
    val blockIds = this.blocks.map { it.id }.toSet()
    val imageIds = this.images.map { it.id }.toSet()
    val pathIds = this.paths.map { it.id }.toSet()
    return copy(
        blocks = blocks.filter { it.id in blockIds },
        images = images.filter { it.id in imageIds },
        paths = paths.filter { it.id in pathIds },
    ).takeIf { it.count > 0 }
}

/** Whether a long-press that ended at [end] never left [slop] points of where it began [origin]: a press on an item, not a marquee. */
internal fun isPress(origin: AnnotationPoint, end: AnnotationPoint, slop: Double): Boolean =
    hypot(end.x - origin.x, end.y - origin.y) <= max(slop, 0.0)
