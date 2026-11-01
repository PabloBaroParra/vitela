package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentFontKind
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.ContentPath
import dev.vitela.pdf.core.ContentTextRun
import dev.vitela.pdf.core.PathPaint
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ContentPathTest {
    private fun path(id: Long, x: Double, y: Double, width: Double, height: Double, paint: PathPaint = PathPaint.Fill) =
        ContentPath(id, 0, AnnotationRect(x, y, width, height), paint)

    @Test
    fun aHairlineRuleCanBeTappedOnItsLine() {
        val rule = path(1, 10.0, 80.0, 60.0, 0.0, PathPaint.Stroke)

        assertEquals(1L, pathAt(listOf(rule), AnnotationPoint(30.0, 80.0), reach = 0.0)?.id)
        assertEquals("the pad is what makes a zero-height box a target", 1L, pathAt(listOf(rule), AnnotationPoint(30.0, 80.0 + PATH_PAD), reach = 0.0)?.id)
        assertNull(pathAt(listOf(rule), AnnotationPoint(30.0, 80.0 + PATH_PAD + 1.0), reach = 0.0))
    }

    @Test
    fun aFilledBoxIsHitAnywhereInside() {
        val box = path(1, 50.0, 20.0, 40.0, 30.0, PathPaint.Fill)

        assertEquals(1L, pathAt(listOf(box), AnnotationPoint(70.0, 35.0), reach = 0.0)?.id)
    }

    @Test
    fun aFrameIsHitOnItsBorderNotInItsCell() {
        // The cell of a table is not the table's border: a tap in it must not open the frame.
        val frame = path(1, 0.0, 0.0, 100.0, 100.0, PathPaint.Stroke)

        assertEquals(1L, pathAt(listOf(frame), AnnotationPoint(2.0, 50.0), reach = 0.0)?.id)
        assertEquals(1L, pathAt(listOf(frame), AnnotationPoint(50.0, 99.0), reach = 0.0)?.id)
        assertNull(pathAt(listOf(frame), AnnotationPoint(50.0, 50.0), reach = 0.0))
    }

    @Test
    fun reachRescuesATapJustOffAPath() {
        val rule = path(1, 10.0, 80.0, 60.0, 0.0, PathPaint.Stroke)
        val tap = AnnotationPoint(30.0, 80.0 + PATH_PAD + 5.0)

        assertNull(pathAt(listOf(rule), tap, reach = 0.0))
        assertEquals(1L, pathAt(listOf(rule), tap, reach = 5.0)?.id)
    }

    @Test
    fun overlappingPathsYieldTheSmallest() {
        val shade = path(1, 0.0, 0.0, 200.0, 200.0, PathPaint.Fill)
        val box = path(2, 20.0, 20.0, 40.0, 40.0, PathPaint.Fill)

        assertEquals(2L, pathAt(listOf(shade, box), AnnotationPoint(30.0, 30.0), reach = 0.0)?.id)
    }

    @Test
    fun aPathYieldsToTextAndImagesTheTapIsInside() {
        val shade = path(1, 0.0, 0.0, 200.0, 200.0)
        val run = ContentTextRun(2, 0, AnnotationRect(10.0, 10.0, 40.0, 12.0), "F1", ContentFontKind.Standard14, "x")
        val image = ContentImage(3, 0, AnnotationRect(100.0, 100.0, 30.0, 30.0), null)

        assertEquals(ContentTarget.Run(run), contentAt(listOf(run), listOf(image), AnnotationPoint(20.0, 15.0), 0.0, listOf(shade)))
        assertEquals(ContentTarget.Image(image), contentAt(listOf(run), listOf(image), AnnotationPoint(110.0, 110.0), 0.0, listOf(shade)))
        assertEquals(ContentTarget.Path(shade), contentAt(listOf(run), listOf(image), AnnotationPoint(150.0, 20.0), 0.0, listOf(shade)))
    }

    @Test
    fun aPathUnderTheFingerBeatsTextOnlyWithinReach() {
        val rule = path(1, 0.0, 80.0, 100.0, 0.0, PathPaint.Stroke)
        val run = ContentTextRun(2, 0, AnnotationRect(10.0, 90.0, 40.0, 12.0), "F1", ContentFontKind.Standard14, "x")

        // On the rule, 10 pt below the run: the rule is under the finger, the run only within reach.
        assertEquals(ContentTarget.Path(rule), contentAt(listOf(run), emptyList(), AnnotationPoint(30.0, 80.0), 12.0, listOf(rule)))
        // Off both, nearer the run: reach has to be used, and the run is the nearer.
        assertEquals(ContentTarget.Run(run), contentAt(listOf(run), emptyList(), AnnotationPoint(30.0, 88.0), 12.0, listOf(rule)))
    }

    @Test
    fun resizingKeepsTheTopLeftCorner() {
        val to = resizedPathRect(AnnotationRect(50.0, 20.0, 40.0, 30.0), 60.0, 10.0)

        assertEquals(AnnotationRect(50.0, 40.0, 60.0, 10.0), to)
    }

    @Test
    fun aFlatAxisStaysFlatWhateverWasTyped() {
        val rule = AnnotationRect(10.0, 80.0, 60.0, 0.0)

        assertEquals(AnnotationRect(10.0, 80.0, 90.0, 0.0), resizedPathRect(rule, 90.0, 7.0))
        assertEquals(AnnotationRect(10.0, 80.0, 60.0, 0.0), resizedPathRect(rule, 60.0, Double.NaN))
    }

    @Test
    fun aSizeThatIsNotPositiveOrFiniteIsRefusedOnTheAxesThatCanChange() {
        val box = AnnotationRect(50.0, 20.0, 40.0, 30.0)

        assertNull(resizedPathRect(box, 0.0, 10.0))
        assertNull(resizedPathRect(box, 10.0, -1.0))
        assertNull(resizedPathRect(box, Double.NaN, 10.0))
        assertNull(resizedPathRect(AnnotationRect(10.0, 80.0, 60.0, 0.0), Double.NaN, 0.0))
    }

    @Test
    fun paddingGrowsABoxOnEverySide() {
        assertEquals(AnnotationRect(8.0, 78.0, 64.0, 4.0), AnnotationRect(10.0, 80.0, 60.0, 0.0).padded(2.0))
        assertTrue(ContentTarget.Path(path(1, 0.0, 0.0, 1.0, 0.0)).drawnBounds.height > 0.0)
    }
}
