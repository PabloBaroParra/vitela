package dev.vitela.pdf.viewer

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentSelection

private val SELECTED_COLOR = Color(0xFF1A59D9)
private val DASHES = floatArrayOf(10f, 6f)

/**
 * Draws a page's multi-selection and the marquee being dragged (B25 T-208):
 * each member highlighted in its drawn box, and a dashed box around them all,
 * the one a drag must start in. While the selection is being [moving] each
 * member's pixels are lifted off [bitmap] — the page as rendered, since only the
 * renderer can paint the words or the picture — and follow the finger, with the
 * highlights and the dashed box. Drawn over the page's own outlines, so a
 * selected member reads as selected whatever kind it is.
 */
internal fun DrawScope.drawSelection(
    selection: ContentSelection?,
    moving: PageDrag?,
    marquee: PageDrag?,
    bitmap: ImageBitmap?,
    placement: PagePlacement,
) {
    if (selection != null) {
        val dx = moving?.let { it.to.x - it.from.x } ?: 0.0
        val dy = moving?.let { it.to.y - it.from.y } ?: 0.0
        fun AnnotationRect.shifted() = copy(x = x + dx, y = y + dy)
        selection.targets.forEach { target ->
            val from = placement.placeRect(target.drawnBounds)
            val to = placement.placeRect(target.drawnBounds.shifted())
            if (moving != null && bitmap != null) drawLifted(bitmap, from, to)
            drawRect(SELECTED_COLOR.copy(alpha = .16f), Offset(to.left.toFloat(), to.top.toFloat()), Size(to.width.toFloat(), to.height.toFloat()))
            drawRect(SELECTED_COLOR, Offset(to.left.toFloat(), to.top.toFloat()), Size(to.width.toFloat(), to.height.toFloat()), style = Stroke(4f))
        }
        val union = placement.placeRect(selection.bounds.shifted().padded(UNION_PAD))
        drawRect(
            SELECTED_COLOR,
            Offset(union.left.toFloat(), union.top.toFloat()),
            Size(union.width.toFloat(), union.height.toFloat()),
            style = Stroke(3f, pathEffect = PathEffect.dashPathEffect(DASHES)),
        )
    }
    if (marquee != null) {
        val box = placement.placeRect(marqueeRect(marquee.from, marquee.to))
        val corner = Offset(box.left.toFloat(), box.top.toFloat())
        val size = Size(box.width.toFloat(), box.height.toFloat())
        drawRect(SELECTED_COLOR.copy(alpha = .11f), corner, size)
        drawRect(SELECTED_COLOR, corner, size, style = Stroke(3f, pathEffect = PathEffect.dashPathEffect(DASHES)))
    }
}

/** How far the dashed box stands off the members it surrounds, in points, so it is not hidden under their own outlines. */
private const val UNION_PAD = 3.0
