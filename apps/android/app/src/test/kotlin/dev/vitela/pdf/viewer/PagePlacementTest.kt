package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.PageRotation
import dev.vitela.pdf.core.PageSize
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The expected values are `pdf_render::selection`'s own tests, case for case.
 * This file is a port of that transform, not a second opinion on it: if one of
 * these disagrees with the Rust test of the same name, the Kotlin is wrong.
 */
class PagePlacementTest {
    /** The unrotated letter page every rotation case is a turn of, at the drawn size that turn gives it. */
    private fun turned(rotation: PageRotation, scale: Double = 1.0): PagePlacement {
        val sideways = rotation == PageRotation.Clockwise90 || rotation == PageRotation.Clockwise270
        return PagePlacement(if (sideways) PageSize(792.0, 612.0, rotation) else PageSize(612.0, 792.0, rotation), scale)
    }

    /** Deliberately tall-thin and cornered: a transform that kept the turn but forgot to swap width for height would still place a centred square. */
    private val cornerRect = AnnotationRect(0.0, 0.0, 10.0, 20.0)

    @Test
    fun placeRect_flipsTheYAxisAndScales() {
        assertEquals(PlacedRect(10.0, 1_364.0, 40.0, 20.0), PagePlacement(PageSize(612.0, 792.0), 2.0).placeRect(AnnotationRect(5.0, 100.0, 20.0, 10.0)))
    }

    @Test
    fun placeRect_carriesACornerAroundWithThePagesTurn() {
        assertEquals(PlacedRect(0.0, 772.0, 10.0, 20.0), turned(PageRotation.None).placeRect(cornerRect))
        assertEquals(PlacedRect(0.0, 0.0, 20.0, 10.0), turned(PageRotation.Clockwise90).placeRect(cornerRect))
        assertEquals(PlacedRect(602.0, 0.0, 10.0, 20.0), turned(PageRotation.Clockwise180).placeRect(cornerRect))
        assertEquals(PlacedRect(772.0, 602.0, 20.0, 10.0), turned(PageRotation.Clockwise270).placeRect(cornerRect))
    }

    @Test
    fun aPlacedRectStaysInsideTheDrawnPageAtEveryTurn() {
        PageRotation.entries.forEach { rotation ->
            val page = turned(rotation)
            val placed = page.placeRect(cornerRect)
            assertTrue("$rotation left", placed.left >= 0.0)
            assertTrue("$rotation top", placed.top >= 0.0)
            assertTrue("$rotation right edge", placed.left + placed.width <= page.page.widthPt)
            assertTrue("$rotation bottom edge", placed.top + placed.height <= page.page.heightPt)
        }
    }

    @Test
    fun pointToPdf_invertsPlaceRect() {
        val page = PagePlacement(PageSize(612.0, 792.0), 1.5)
        val placed = page.placeRect(AnnotationRect(5.0, 100.0, 20.0, 10.0))
        // The placed top-left is the rect's PDF-space top-left: its y + height edge.
        val pdf = page.pointToPdf(placed.left, placed.top)
        assertEquals(5.0, pdf.x, 1e-9)
        assertEquals(110.0, pdf.y, 1e-9)
    }

    /** A press that lands on an outline must hit the content that outline was drawn for, on every turn. */
    @Test
    fun pointToPdf_invertsPlacePointAtEveryTurn() {
        val point = AnnotationPoint(137.0, 431.0)
        PageRotation.entries.forEach { rotation ->
            val page = turned(rotation, scale = 1.5)
            val drawn = page.placePoint(point)
            val pdf = page.pointToPdf(drawn.x, drawn.y)
            assertEquals("$rotation x", point.x, pdf.x, 1e-9)
            assertEquals("$rotation y", point.y, pdf.y, 1e-9)
        }
    }

    @Test
    fun pointToPdf_fallsBackToOneToOneForAnUnusableScale() {
        assertEquals(AnnotationPoint(30.0, 700.0), PagePlacement(PageSize(612.0, 792.0), 0.0).pointToPdf(30.0, 92.0))
        assertEquals(AnnotationPoint(30.0, 700.0), PagePlacement(PageSize(612.0, 792.0), Double.NaN).pointToPdf(30.0, 92.0))
    }

    /** The bug this type exists for: a tap near the drawn top-left of a quarter-turned page is near the PDF origin, not near its top edge. */
    @Test
    fun aTapOnAQuarterTurnedPageLandsInTheUnrotatedSpace() {
        assertEquals(AnnotationPoint(20.0, 10.0), turned(PageRotation.Clockwise90).pointToPdf(10.0, 20.0))
    }

    @Test
    fun theUnrotatedSizeUndoesAQuarterTurnsSwap() {
        assertEquals(PageSize(612.0, 792.0), PageSize(792.0, 612.0, PageRotation.Clockwise90).unrotated)
        assertEquals(PageSize(612.0, 792.0), PageSize(612.0, 792.0, PageRotation.Clockwise180).unrotated)
    }
}
