package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.PageRotation
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.TextRect

/** A rect in the page slot's drawing space: top-left origin, y growing downwards, in pixels. */
internal data class PlacedRect(val left: Double, val top: Double, val width: Double, val height: Double)

/**
 * Where page-space geometry lands on a page as it is drawn, and back.
 *
 * A page carrying `/Rotate` is rasterized turned, and [page] is its size with
 * the turn applied. Everything *on* the page — text runs, images, fields,
 * annotations, search and selection rects — stays in the page's unrotated
 * space, because a `/Rotate` is a viewing instruction and moves nothing in the
 * file. A bare y-flip puts every overlay, and every tap, in the wrong place
 * the moment a page is turned.
 *
 * This is a port of `pdf_render::selection::{place_rect, place_point,
 * point_to_pdf}`, which `pdf-ffi` also exports (`placement.rs`) so shells need
 * no copy of it. Android keeps one anyway: these run for every outline on every
 * Compose frame, where a JNA crossing per rect is a cost the phone pays, and a
 * JVM unit test cannot load the native library at all. `PagePlacementTest`
 * pins it to the Rust tests' values, case for case.
 *
 * The forward transform works from the **drawn** size, the inverse from the
 * **unrotated** one; mixing those up is the easy mistake.
 */
internal data class PagePlacement(val page: PageSize, val scale: Double) {
    /** A scale that is not finite and positive would divide a tap into infinity; fall back to 1:1. */
    private val usableScale get() = if (scale.isFinite() && scale > 0.0) scale else 1.0

    /** Places a page-space rect on the page as drawn; on a quarter-turned page its width and height swap. */
    fun placeRect(x: Double, y: Double, width: Double, height: Double): PlacedRect {
        val drawnWidth = page.widthPt
        val drawnHeight = page.heightPt
        // Each arm places the rect's *near* corner under the turn — the PDF-space
        // corner that ends up top-left once the page is turned.
        val placed = when (page.rotation) {
            PageRotation.None -> PlacedRect(x, drawnHeight - (y + height), width, height)
            PageRotation.Clockwise90 -> PlacedRect(y, x, height, width)
            PageRotation.Clockwise180 -> PlacedRect(drawnWidth - (x + width), y, width, height)
            PageRotation.Clockwise270 -> PlacedRect(drawnWidth - (y + height), drawnHeight - (x + width), height, width)
        }
        return PlacedRect(placed.left * scale, placed.top * scale, placed.width * scale, placed.height * scale)
    }

    fun placeRect(rect: AnnotationRect) = placeRect(rect.x, rect.y, rect.width, rect.height)

    fun placeRect(rect: TextRect) = placeRect(rect.x, rect.y, rect.width, rect.height)

    /** [placeRect] for a bare point — an ink vertex, a rule's end, a resize handle. */
    fun placePoint(point: AnnotationPoint): AnnotationPoint {
        val (x, y) = point
        val drawn = when (page.rotation) {
            PageRotation.None -> AnnotationPoint(x, page.heightPt - y)
            PageRotation.Clockwise90 -> AnnotationPoint(y, x)
            PageRotation.Clockwise180 -> AnnotationPoint(page.widthPt - x, y)
            PageRotation.Clockwise270 -> AnnotationPoint(page.widthPt - y, page.heightPt - x)
        }
        return AnnotationPoint(drawn.x * scale, drawn.y * scale)
    }

    /** The inverse of [placePoint]: a pointer position on the drawn page, back into page space. */
    fun pointToPdf(x: Double, y: Double): AnnotationPoint {
        val px = x / usableScale
        val py = y / usableScale
        val unrotated = page.unrotated
        return when (page.rotation) {
            PageRotation.None -> AnnotationPoint(px, unrotated.heightPt - py)
            PageRotation.Clockwise90 -> AnnotationPoint(py, px)
            PageRotation.Clockwise180 -> AnnotationPoint(unrotated.widthPt - px, py)
            PageRotation.Clockwise270 -> AnnotationPoint(unrotated.widthPt - py, unrotated.heightPt - px)
        }
    }
}
