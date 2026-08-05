package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationColor
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.TextRect

internal val DEFAULT_ANNOTATION_COLOR = AnnotationColor(255, 220, 0)
private const val MIN_RECT_PT = 4.0
private const val RULE_HEIGHT_PT = 2.0
private const val GROW_FACTOR = 1.25
internal const val HANDLE_REACH_DP = 24.0

enum class AnnotationTool(val kind: AnnotationKind) {
    Pointer(AnnotationKind.Highlight), Highlight(AnnotationKind.Highlight), Underline(AnnotationKind.Underline),
    Strikeout(AnnotationKind.Strikeout), Ink(AnnotationKind.Ink), Shape(AnnotationKind.Shape),
    TextNote(AnnotationKind.TextNote), Stamp(AnnotationKind.Stamp),
}

enum class HandleCorner { BottomLeft, BottomRight, TopLeft, TopRight }
sealed interface DragMode { data object Move : DragMode; data class Resize(val corner: HandleCorner) : DragMode }
data class TextSelection(val pageIndex: Int, val rects: List<TextRect>)
internal data class AnnotationControls(
    val canCreate: Boolean, val canMove: Boolean, val canResize: Boolean, val canRestyle: Boolean,
    val canGrow: Boolean, val canUndo: Boolean, val canRedo: Boolean,
) {
    companion object { val disabled = AnnotationControls(false, false, false, false, false, false, false) }
}

internal fun screenToPdf(point: AnnotationPoint, pageHeightPt: Double, scale: Double) =
    AnnotationPoint(point.x / scale, pageHeightPt - point.y / scale)

internal fun pdfToScreen(point: AnnotationPoint, pageHeightPt: Double, scale: Double) =
    AnnotationPoint(point.x * scale, (pageHeightPt - point.y) * scale)

internal fun placementAnnotation(tool: AnnotationTool, pageIndex: Int, origin: AnnotationPoint, current: AnnotationPoint, points: List<AnnotationPoint> = emptyList()): Annotation {
    val rect = AnnotationRect(
        minOf(origin.x, current.x),
        if (tool == AnnotationTool.Underline || tool == AnnotationTool.Strikeout) origin.y else minOf(origin.y, current.y),
        maxOf(MIN_RECT_PT, kotlin.math.abs(current.x - origin.x)),
        if (tool == AnnotationTool.Underline || tool == AnnotationTool.Strikeout) RULE_HEIGHT_PT else maxOf(MIN_RECT_PT, kotlin.math.abs(current.y - origin.y)),
    )
    return Annotation(0, pageIndex, tool.kind, if (tool == AnnotationTool.Ink) null else rect, if (tool == AnnotationTool.TextNote || tool == AnnotationTool.Stamp) null else DEFAULT_ANNOTATION_COLOR, points)
}

internal fun markupTextSelection(tool: AnnotationTool, selection: TextSelection): List<Annotation> =
    if (tool !in setOf(AnnotationTool.Highlight, AnnotationTool.Underline, AnnotationTool.Strikeout)) emptyList() else selection.rects.map {
        Annotation(0, selection.pageIndex, tool.kind, AnnotationRect(it.x, it.y, it.width, it.height), DEFAULT_ANNOTATION_COLOR)
    }

internal val Annotation.bounds: AnnotationRect?
    get() = rect ?: points.takeIf { it.isNotEmpty() }?.let { stroke ->
        val minX = stroke.minOf { it.x }; val maxX = stroke.maxOf { it.x }
        val minY = stroke.minOf { it.y }; val maxY = stroke.maxOf { it.y }
        AnnotationRect(minX, minY, maxX - minX, maxY - minY)
    }

internal val Annotation.supportsResize get() = rect != null
internal val Annotation.supportsRestyle get() = kind in setOf(AnnotationKind.Highlight, AnnotationKind.Underline, AnnotationKind.Strikeout, AnnotationKind.Ink, AnnotationKind.Shape)

internal fun handleReachPoints(screenReachDp: Double, density: Double, pageScale: Double): Double =
    screenReachDp * density / pageScale

internal fun annotationAt(annotations: List<Annotation>, pageIndex: Int, point: AnnotationPoint): Annotation? =
    annotations.lastOrNull { annotation ->
        annotation.pageIndex == pageIndex && annotation.bounds?.let { bounds ->
            point.x in bounds.x..(bounds.x + bounds.width) && point.y in bounds.y..(bounds.y + bounds.height)
        } == true
    }

internal fun dragModeAt(annotation: Annotation, point: AnnotationPoint, reach: Double): DragMode? {
    val rect = annotation.bounds ?: return null
    if (annotation.supportsResize) HandleCorner.entries.firstOrNull { corner ->
        val at = cornerPoint(rect, corner)
        kotlin.math.abs(point.x - at.x) <= reach && kotlin.math.abs(point.y - at.y) <= reach
    }?.let { return DragMode.Resize(it) }
    return if (point.x in rect.x..(rect.x + rect.width) && point.y in rect.y..(rect.y + rect.height)) DragMode.Move else null
}

internal fun resizedRect(rect: AnnotationRect, corner: HandleCorner, point: AnnotationPoint): AnnotationRect {
    val anchor = cornerPoint(rect, opposite(corner))
    return AnnotationRect(minOf(anchor.x, point.x), minOf(anchor.y, point.y), maxOf(MIN_RECT_PT, kotlin.math.abs(point.x - anchor.x)), maxOf(MIN_RECT_PT, kotlin.math.abs(point.y - anchor.y)))
}

internal fun grownRect(rect: AnnotationRect) = rect.copy(width = rect.width * GROW_FACTOR, height = rect.height * GROW_FACTOR)
internal fun movedRect(rect: AnnotationRect?, dx: Double, dy: Double): AnnotationRect? = rect?.copy(x = rect.x + dx, y = rect.y + dy)

/** Preview of [Annotation] shifted by (dx, dy) — mirrors what a committed [AnnotationEdit.Move] produces, for drawing a live drag preview before the edit is applied. */
internal fun Annotation.translated(dx: Double, dy: Double): Annotation =
    copy(rect = movedRect(rect, dx, dy), points = points.map { AnnotationPoint(it.x + dx, it.y + dy) })
internal fun annotationControls(editingAllowed: Boolean, selected: Annotation?, canUndo: Boolean, canRedo: Boolean): AnnotationControls =
    if (!editingAllowed) AnnotationControls.disabled else AnnotationControls(true, selected != null, selected?.supportsResize == true, selected?.supportsRestyle == true, selected?.supportsResize == true, canUndo, canRedo)

private fun cornerPoint(rect: AnnotationRect, corner: HandleCorner) = when (corner) {
    HandleCorner.BottomLeft -> AnnotationPoint(rect.x, rect.y)
    HandleCorner.BottomRight -> AnnotationPoint(rect.x + rect.width, rect.y)
    HandleCorner.TopLeft -> AnnotationPoint(rect.x, rect.y + rect.height)
    HandleCorner.TopRight -> AnnotationPoint(rect.x + rect.width, rect.y + rect.height)
}
private fun opposite(corner: HandleCorner) = when (corner) {
    HandleCorner.BottomLeft -> HandleCorner.TopRight; HandleCorner.BottomRight -> HandleCorner.TopLeft
    HandleCorner.TopLeft -> HandleCorner.BottomRight; HandleCorner.TopRight -> HandleCorner.BottomLeft
}
