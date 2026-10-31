package dev.vitela.pdf.viewer

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentPath
import dev.vitela.pdf.core.ContentTextBlock

/**
 * Outlines each of a page's vector paths in orange, apart from the blue of text
 * and the green of images. Each is drawn in its [padded] box, the box
 * [pathAt] hits, so a hairline rule — a box of height zero — has an outline at
 * all. The [moving] path, the one an armed move will place, is drawn heavier
 * and, while it is dragged, at [dragged] — the box it would land in. Called
 * before the images and the text, so a caption's outline stays on top of the
 * frame it sits in.
 */
internal fun DrawScope.drawPathOutlines(paths: List<ContentPath>, moving: ContentPath?, dragged: AnnotationRect?, placement: PagePlacement) {
    paths.forEach { path ->
        val isMoving = moving?.id == path.id
        val bounds = if (isMoving && dragged != null) dragged else path.bounds
        val placed = placement.placeRect(bounds.padded(PATH_PAD))
        drawRect(
            if (isMoving) Color(0xFFE08A1E) else Color(0x99E08A1E),
            Offset(placed.left.toFloat(), placed.top.toFloat()),
            Size(placed.width.toFloat(), placed.height.toFloat()),
            style = Stroke(if (isMoving) 5f else 2f),
        )
    }
}

/**
 * Outlines each of a page's text blocks in blue — one box per line, not one per
 * run, so a phrase the file paints in pieces is the one target a tap opens.
 * Solid where the block keeps its font, purple and dashed where a retype swaps
 * in a standard one: otherwise nothing tells the two apart before the reader
 * has typed. The [moving] block, the one an armed move will place, is drawn
 * heavier and, while it is dragged, at [dragged] — the box it would land in.
 */
internal fun DrawScope.drawTextOutlines(blocks: List<ContentTextBlock>, moving: ContentTextBlock?, dragged: AnnotationRect?, placement: PagePlacement) {
    blocks.forEach { block ->
        val isMoving = moving?.id == block.id
        val bounds = if (isMoving && dragged != null) dragged else block.bounds
        val placed = placement.placeRect(bounds)
        drawRect(
            if (block.substitutesFont) Color(if (isMoving) 0xFFAA5ADC else 0x99AA5ADC) else Color(if (isMoving) 0xFF2878EB else 0x992878EB),
            Offset(placed.left.toFloat(), placed.top.toFloat()),
            Size(placed.width.toFloat(), placed.height.toFloat()),
            style = Stroke(if (isMoving) 5f else 2f, pathEffect = if (block.substitutesFont) PathEffect.dashPathEffect(floatArrayOf(6f, 4f)) else null),
        )
    }
}
