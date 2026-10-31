package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentFontKind
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.ContentTextBlock
import dev.vitela.pdf.core.ContentTextRun
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ContentEditTest {
    private fun run(id: Long, x: Double, y: Double, width: Double, height: Double, kind: ContentFontKind = ContentFontKind.Standard14) =
        ContentTextRun(id, 0, AnnotationRect(x, y, width, height), "F1", kind, "run $id")

    /** A block of one run, named by its id. */
    private fun block(id: Long, x: Double, y: Double, width: Double, height: Double, kind: ContentFontKind = ContentFontKind.Standard14) =
        run(id, x, y, width, height, kind).let { ContentTextBlock(listOf(it), it.bounds, it.text) }

    @Test
    fun aTapInsideABlockFindsIt() {
        val runs = listOf(block(1, 10.0, 100.0, 50.0, 12.0), block(2, 10.0, 80.0, 50.0, 12.0))

        assertEquals(2L, textBlockAt(runs, AnnotationPoint(30.0, 85.0), reach = 0.0)?.id)
    }

    @Test
    fun overlappingBlocksYieldTheSmallest() {
        // A heading run drawn over a wider line: the finger meant the one it can see the edges of.
        val runs = listOf(block(1, 0.0, 100.0, 200.0, 30.0), block(2, 20.0, 105.0, 40.0, 12.0))

        assertEquals(2L, textBlockAt(runs, AnnotationPoint(30.0, 110.0), reach = 0.0)?.id)
    }

    @Test
    fun aTapJustOutsideABlockFindsItWithinReach() {
        val runs = listOf(block(1, 10.0, 100.0, 50.0, 12.0))

        assertEquals(1L, textBlockAt(runs, AnnotationPoint(30.0, 96.0), reach = 5.0)?.id)
        assertNull(textBlockAt(runs, AnnotationPoint(30.0, 96.0), reach = 0.0))
    }

    @Test
    fun withinReachTheNearestBlockWins() {
        val runs = listOf(block(1, 10.0, 100.0, 50.0, 12.0), block(2, 10.0, 80.0, 50.0, 12.0))

        // 3pt above run 2's top edge (92), 5pt below run 1's bottom edge (100).
        assertEquals(2L, textBlockAt(runs, AnnotationPoint(30.0, 95.0), reach = 10.0)?.id)
    }

    @Test
    fun aTapFarFromEveryBlockFindsNothing() {
        assertNull(textBlockAt(listOf(block(1, 10.0, 100.0, 50.0, 12.0)), AnnotationPoint(150.0, 10.0), reach = 5.0))
    }

    @Test
    fun onlyACompositeFontIsSubstituted() {
        assertFalse(block(1, 0.0, 0.0, 1.0, 1.0, ContentFontKind.Standard14).substitutesFont)
        assertFalse(block(1, 0.0, 0.0, 1.0, 1.0, ContentFontKind.EmbeddedSimple).substitutesFont)
        assertTrue(block(1, 0.0, 0.0, 1.0, 1.0, ContentFontKind.EmbeddedComposite).substitutesFont)
    }

    @Test
    fun aBlockIsHitOnAnyOfItsRunsAndInTheGapBetween() {
        val one = run(1, 10.0, 100.0, 30.0, 12.0)
        val two = run(2, 42.0, 100.0, 40.0, 12.0)
        val line = ContentTextBlock(listOf(one, two), AnnotationRect(10.0, 100.0, 72.0, 12.0), "Two runs")

        assertEquals(line, textBlockAt(listOf(line), AnnotationPoint(15.0, 105.0), reach = 0.0))
        assertEquals(line, textBlockAt(listOf(line), AnnotationPoint(70.0, 105.0), reach = 0.0))
        assertEquals("the word gap belongs to the line", line, textBlockAt(listOf(line), AnnotationPoint(41.0, 105.0), reach = 0.0))
    }

    @Test
    fun aBlockIsNamedAndPlacedByItsFirstRun() {
        val one = run(7, 10.0, 100.0, 30.0, 12.0).copy(pageIndex = 3)
        val line = ContentTextBlock(listOf(one, run(8, 42.0, 100.0, 40.0, 12.0)), AnnotationRect(10.0, 100.0, 72.0, 12.0), "x y")

        assertEquals(7L, line.id)
        assertEquals(3, line.pageIndex)
    }

    @Test
    fun aBlockSubstitutesItsFontWhenItsFirstRunDoes() {
        val composite = run(1, 10.0, 100.0, 30.0, 12.0, ContentFontKind.EmbeddedComposite)
        val plain = run(2, 42.0, 100.0, 40.0, 12.0)

        assertTrue(ContentTextBlock(listOf(composite, plain), composite.bounds, "a b").substitutesFont)
        assertFalse(ContentTextBlock(listOf(plain, composite), plain.bounds, "b a").substitutesFont)
    }

    private fun image(id: Long, x: Double, y: Double, width: Double, height: Double) =
        ContentImage(id, 0, AnnotationRect(x, y, width, height), "Im$id")

    @Test
    fun textOverAnImageIsWhatATapOnItMeans() {
        // A caption printed over a photo: the photo is still reachable around it.
        val runs = listOf(block(1, 20.0, 40.0, 40.0, 12.0))
        val images = listOf(image(9, 0.0, 0.0, 100.0, 100.0))

        assertEquals(ContentTarget.Block(runs[0]), contentAt(runs, images, AnnotationPoint(30.0, 45.0), reach = 5.0))
        assertEquals(ContentTarget.Image(images[0]), contentAt(runs, images, AnnotationPoint(80.0, 80.0), reach = 5.0))
    }

    @Test
    fun aTapInsideAnImageBeatsTextMerelyWithinReach() {
        val runs = listOf(block(1, 10.0, 100.0, 50.0, 12.0))
        val images = listOf(image(9, 10.0, 60.0, 50.0, 38.0))

        // 3pt below the run, inside the image.
        assertEquals(ContentTarget.Image(images[0]), contentAt(runs, images, AnnotationPoint(30.0, 97.0), reach = 5.0))
    }

    @Test
    fun overlappingImagesYieldTheSmallest() {
        val images = listOf(image(8, 0.0, 0.0, 200.0, 200.0), image(9, 20.0, 20.0, 40.0, 40.0))

        assertEquals(ContentTarget.Image(images[1]), contentAt(emptyList(), images, AnnotationPoint(30.0, 30.0), reach = 0.0))
    }

    @Test
    fun anImageIsFoundWithinReachWhenNoTextIs() {
        val images = listOf(image(9, 10.0, 10.0, 20.0, 20.0))

        assertEquals(ContentTarget.Image(images[0]), contentAt(emptyList(), images, AnnotationPoint(33.0, 20.0), reach = 5.0))
        assertNull(contentAt(emptyList(), images, AnnotationPoint(33.0, 20.0), reach = 0.0))
    }

    @Test
    fun aResizedImageKeepsItsTopLeftCorner() {
        assertEquals(AnnotationRect(10.0, 35.0, 60.0, 15.0), resizedImageRect(AnnotationRect(10.0, 20.0, 30.0, 30.0), 60.0, 15.0))
    }

    @Test
    fun aResizedImageMayOutgrowThePage() {
        // Unlike a form field, an image may already hang off the page; the size typed is the size sent.
        assertEquals(AnnotationRect(10.0, -950.0, 2000.0, 1000.0), resizedImageRect(AnnotationRect(10.0, 20.0, 30.0, 30.0), 2000.0, 1000.0))
    }

    @Test
    fun onlyAFinitePositiveSizeResizesAnImage() {
        val bounds = AnnotationRect(10.0, 20.0, 30.0, 30.0)

        assertNull(resizedImageRect(bounds, 0.0, 10.0))
        assertNull(resizedImageRect(bounds, 10.0, -1.0))
        assertNull(resizedImageRect(bounds, Double.NaN, 10.0))
        assertNull(resizedImageRect(bounds, 10.0, Double.POSITIVE_INFINITY))
    }

    @Test
    fun aMovedImagePutsItsTopLeftCornerAtTheTapKeepingItsSize() {
        assertEquals(AnnotationRect(50.0, 150.0, 30.0, 30.0), movedRect(AnnotationRect(10.0, 20.0, 30.0, 30.0), AnnotationPoint(50.0, 180.0)))
    }

    @Test
    fun aDragCarriesTheTopLeftCornerByTheFingersTravel() {
        // Grabbed mid-box: the corner keeps its distance from the finger, it does not jump to it.
        val corner = draggedCorner(AnnotationRect(10.0, 20.0, 30.0, 12.0), from = AnnotationPoint(25.0, 26.0), to = AnnotationPoint(65.0, 6.0))

        assertEquals(AnnotationPoint(50.0, 12.0), corner)
        assertEquals(AnnotationRect(50.0, 0.0, 30.0, 12.0), movedRect(AnnotationRect(10.0, 20.0, 30.0, 12.0), corner))
    }

    @Test
    fun aDragGrabsTheArmedBoxOnlyWithinReach() {
        val bounds = AnnotationRect(10.0, 20.0, 30.0, 12.0)

        assertTrue(grabs(bounds, AnnotationPoint(20.0, 25.0), reach = 0.0))
        assertTrue(grabs(bounds, AnnotationPoint(45.0, 25.0), reach = 6.0))
        assertFalse(grabs(bounds, AnnotationPoint(80.0, 25.0), reach = 6.0))
    }

    @Test
    fun aMovedImageMayHangOffThePage() {
        assertEquals(AnnotationRect(-5.0, -25.0, 30.0, 30.0), movedRect(AnnotationRect(10.0, 20.0, 30.0, 30.0), AnnotationPoint(-5.0, 5.0)))
    }

    @Test
    fun aNewLineHangsBelowTheTapAtItsSize() {
        // The core reads the left edge, the bottom edge and the height — the font size.
        assertEquals(AnnotationRect(50.0, 160.0, 20.0, 20.0), insertedTextRect(AnnotationPoint(50.0, 180.0), 20.0))
    }

    @Test
    fun aNewLineIsBetweenOneAndSeventyTwoPoints() {
        val at = AnnotationPoint(50.0, 180.0)

        assertEquals(1.0, insertedTextRect(at, 1.0)?.height)
        assertEquals(72.0, insertedTextRect(at, 72.0)?.height)
        assertNull(insertedTextRect(at, 0.5))
        assertNull(insertedTextRect(at, 72.5))
        assertNull(insertedTextRect(at, Double.NaN))
    }

    @Test
    fun onlyOneNonBlankLineIsInsertable() {
        assertTrue(insertableText("Hello"))
        assertFalse(insertableText(""))
        assertFalse(insertableText("   "))
        assertFalse(insertableText("two\nlines"))
        assertFalse(insertableText("two\rlines"))
    }
}
