package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentPath
import dev.vitela.pdf.core.PathPaint
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min

/**
 * The shape dialog, for a vector path — a table border, a rule, a shaded box.
 * [width] and [height] start as the path's, in points, and after a refusal are
 * what the reader typed — kept with the [error], like [ImageResizer.width]. A
 * flat axis (a rule's height) reads as zero and is never read back: see
 * [resizedPathRect].
 */
data class PathShaper(
    val path: ContentPath,
    val width: String = pointsText(path.bounds.width),
    val height: String = pointsText(path.bounds.height),
    val error: String? = null,
)

// Wording follows the Windows shell's: a path is a "shape" to the reader.
internal const val PATH_RESIZED = "Shape resized. Save to keep the change."
internal const val PATH_SIZE_INVALID = "Shape dimensions must be finite and greater than zero."
internal const val PATH_MOVED = "Shape moved. Save to keep the change."
internal const val PATH_POSITION_UNCHANGED = "Shape position unchanged."
internal const val PATH_DELETED = "Shape deleted. Save to keep the change."

/** What an armed move asks for; the shape stays on its own page. */
internal fun pathMovePrompt(pageIndex: Int) = "Drag the shape, or tap page ${pageIndex + 1} where its top-left corner should go."

/**
 * How far past its box a path is still under the finger, in points. The core's
 * box covers the geometry only, so a hairline rule is as tall as nothing: the
 * pad is what makes it a target, and what the outline is drawn with.
 */
internal const val PATH_PAD = 4.0

/**
 * The path a tap at [point] meant, or null: the nearest within [reach] points
 * of its padded box — of its edges, for an outline ([PathPaint.Stroke]), since
 * a frame's cell is not the frame, and the Windows shell opens a table's border
 * only where the border is. Among paths equally near (a box drawn inside a
 * shaded one), the smallest wins: it is the one whose edges the reader can see.
 */
internal fun pathAt(paths: List<ContentPath>, point: AnnotationPoint, reach: Double): ContentPath? =
    paths.map { it to pathDistance(it, point) }
        .filter { (_, distance) -> distance <= reach }
        .minWithOrNull(compareBy<Pair<ContentPath, Double>> { it.second }.thenBy { it.first.bounds.width * it.first.bounds.height })
        ?.first

/** How far [point] is from [path], once [PATH_PAD] is allowed: zero anywhere the path counts as hit. */
private fun pathDistance(path: ContentPath, point: AnnotationPoint): Double {
    val box = path.bounds
    val dx = max(max(box.x - point.x, point.x - (box.x + box.width)), 0.0)
    val dy = max(max(box.y - point.y, point.y - (box.y + box.height)), 0.0)
    val outside = hypot(dx, dy)
    val distance = if (path.paint == PathPaint.Stroke && outside == 0.0) {
        min(min(point.x - box.x, box.x + box.width - point.x), min(point.y - box.y, box.y + box.height - point.y))
    } else {
        outside
    }
    return max(distance - PATH_PAD, 0.0)
}

/**
 * The box a path at [bounds] fills at [width] by [height] points, or null for
 * a size that cannot be sent. Its top-left corner stays put, as an image's
 * does ([resizedImageRect]). An axis that is flat — a rule's height — is
 * moved, never scaled, so it stays flat whatever was typed for it, and only
 * the axes that can change are validated.
 */
internal fun resizedPathRect(bounds: AnnotationRect, width: Double, height: Double): AnnotationRect? {
    val flatWidth = bounds.width <= 0.0
    val flatHeight = bounds.height <= 0.0
    if (!flatWidth && (!width.isFinite() || width <= 0.0)) return null
    if (!flatHeight && (!height.isFinite() || height <= 0.0)) return null
    val newWidth = if (flatWidth) 0.0 else width
    val newHeight = if (flatHeight) 0.0 else height
    return AnnotationRect(bounds.x, bounds.y + bounds.height - newHeight, newWidth, newHeight)
}

/** [this] grown by [by] points on every side. */
internal fun AnnotationRect.padded(by: Double) = AnnotationRect(x - by, y - by, width + 2 * by, height + 2 * by)

/**
 * The box a run, an image or a path is drawn in: a path's padded, so a
 * hairline has something to outline and lift off the page.
 */
internal val ContentTarget.drawnBounds: AnnotationRect
    get() = if (this is ContentTarget.Path) bounds.padded(PATH_PAD) else bounds
