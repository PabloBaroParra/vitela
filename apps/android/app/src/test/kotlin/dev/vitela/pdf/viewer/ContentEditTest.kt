package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentFontKind
import dev.vitela.pdf.core.ContentTextRun
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ContentEditTest {
    private fun run(id: Long, x: Double, y: Double, width: Double, height: Double, kind: ContentFontKind = ContentFontKind.Standard14) =
        ContentTextRun(id, 0, AnnotationRect(x, y, width, height), "F1", kind, "run $id")

    @Test
    fun aTapInsideARunFindsIt() {
        val runs = listOf(run(1, 10.0, 100.0, 50.0, 12.0), run(2, 10.0, 80.0, 50.0, 12.0))

        assertEquals(2L, textRunAt(runs, AnnotationPoint(30.0, 85.0), reach = 0.0)?.id)
    }

    @Test
    fun overlappingRunsYieldTheSmallest() {
        // A heading run drawn over a wider line: the finger meant the one it can see the edges of.
        val runs = listOf(run(1, 0.0, 100.0, 200.0, 30.0), run(2, 20.0, 105.0, 40.0, 12.0))

        assertEquals(2L, textRunAt(runs, AnnotationPoint(30.0, 110.0), reach = 0.0)?.id)
    }

    @Test
    fun aTapJustOutsideARunFindsItWithinReach() {
        val runs = listOf(run(1, 10.0, 100.0, 50.0, 12.0))

        assertEquals(1L, textRunAt(runs, AnnotationPoint(30.0, 96.0), reach = 5.0)?.id)
        assertNull(textRunAt(runs, AnnotationPoint(30.0, 96.0), reach = 0.0))
    }

    @Test
    fun withinReachTheNearestRunWins() {
        val runs = listOf(run(1, 10.0, 100.0, 50.0, 12.0), run(2, 10.0, 80.0, 50.0, 12.0))

        // 3pt above run 2's top edge (92), 5pt below run 1's bottom edge (100).
        assertEquals(2L, textRunAt(runs, AnnotationPoint(30.0, 95.0), reach = 10.0)?.id)
    }

    @Test
    fun aTapFarFromEveryRunFindsNothing() {
        assertNull(textRunAt(listOf(run(1, 10.0, 100.0, 50.0, 12.0)), AnnotationPoint(150.0, 10.0), reach = 5.0))
    }

    @Test
    fun onlyACompositeFontIsSubstituted() {
        assertFalse(run(1, 0.0, 0.0, 1.0, 1.0, ContentFontKind.Standard14).substitutesFont)
        assertFalse(run(1, 0.0, 0.0, 1.0, 1.0, ContentFontKind.EmbeddedSimple).substitutesFont)
        assertTrue(run(1, 0.0, 0.0, 1.0, 1.0, ContentFontKind.EmbeddedComposite).substitutesFont)
    }
}
