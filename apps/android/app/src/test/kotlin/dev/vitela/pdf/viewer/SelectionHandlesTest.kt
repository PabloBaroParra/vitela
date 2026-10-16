package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.TextRect
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SelectionHandlesTest {
    private val placement = PagePlacement(PageSize(612.0, 792.0), 1.0)

    /** Two lines: the first starts at x 100, the second ends at x 130. */
    private val twoLines = TextSelection(0, listOf(TextRect(100.0, 700.0, 200.0, 12.0), TextRect(50.0, 680.0, 80.0, 12.0)), "two lines", 3, 20)

    @Test
    fun theStartHandleHangsUnderTheFirstLinesLeftEdge() {
        val start = selectionHandleSpots(twoLines, placement, knobRadiusPx = 10.0).first { it.handle == SelectionHandle.Start }

        assertEquals(AnnotationPoint(100.0, 706.0), start.caretPoint)
        assertEquals(AnnotationPoint(100.0, 80.0), start.stemTop)
        assertEquals(AnnotationPoint(100.0, 92.0), start.foot)
        assertEquals(AnnotationPoint(100.0, 102.0), start.knob)
    }

    @Test
    fun theEndHandleHangsUnderTheLastLinesRightEdge() {
        val end = selectionHandleSpots(twoLines, placement, knobRadiusPx = 10.0).first { it.handle == SelectionHandle.End }

        assertEquals(AnnotationPoint(130.0, 686.0), end.caretPoint)
        assertEquals(AnnotationPoint(130.0, 112.0), end.foot)
        assertEquals(AnnotationPoint(130.0, 122.0), end.knob)
    }

    @Test
    fun aSelectionWithoutRectsHasNoHandles() {
        assertTrue(selectionHandleSpots(TextSelection(0, emptyList(), "", 0, 0), placement, 10.0).isEmpty())
    }

    @Test
    fun aTouchWithinReachOfAKnobGrabsIt() {
        val spots = selectionHandleSpots(twoLines, placement, 10.0)

        assertEquals(SelectionHandle.Start, handleAt(spots, 110.0, 110.0, reachPx = 24.0))
        assertEquals(SelectionHandle.End, handleAt(spots, 125.0, 130.0, reachPx = 24.0))
    }

    @Test
    fun aTouchOutOfReachGrabsNothing() {
        assertNull(handleAt(selectionHandleSpots(twoLines, placement, 10.0), 300.0, 300.0, reachPx = 24.0))
    }

    @Test
    fun theToolbarSitsAboveTheSelectionsFirstLine() {
        val spots = selectionHandleSpots(twoLines, placement, 10.0)

        // Above the start stem (y 80) by its 40 px height and an 8 px gap, left-aligned to the start.
        assertEquals(AnnotationPoint(100.0, 32.0), selectionToolbarOrigin(spots, 200.0, 40.0, 612.0, 792.0, gapPx = 8.0, knobRadiusPx = 10.0))
    }

    @Test
    fun aToolbarWithNoRoomAboveDropsBelowTheEndKnob() {
        val spots = selectionHandleSpots(twoLines, placement, 10.0)

        // End knob centre y 122 + radius 10 + gap 8.
        assertEquals(AnnotationPoint(100.0, 140.0), selectionToolbarOrigin(spots, 200.0, 90.0, 612.0, 792.0, gapPx = 8.0, knobRadiusPx = 10.0))
    }

    @Test
    fun theToolbarIsKeptInsideThePageWidth() {
        val spots = selectionHandleSpots(twoLines, placement, 10.0)

        // Wider than the page: pinned to its left edge rather than pushed off it.
        assertEquals(0.0, selectionToolbarOrigin(spots, 700.0, 40.0, 612.0, 792.0, 8.0, 10.0)!!.x, 0.0)
        val nearRightEdge = selectionHandleSpots(TextSelection(0, listOf(TextRect(550.0, 700.0, 50.0, 12.0)), "edge", 0, 4), placement, 10.0)
        assertEquals(412.0, selectionToolbarOrigin(nearRightEdge, 200.0, 40.0, 612.0, 792.0, 8.0, 10.0)!!.x, 0.0)
    }

    @Test
    fun aSelectionWithoutHandlesHasNoToolbar() {
        assertNull(selectionToolbarOrigin(emptyList(), 200.0, 40.0, 612.0, 792.0, 8.0, 10.0))
    }

    @Test
    fun whenBothKnobsAreInReachTheNearerOneWins() {
        // One short word: the knobs sit 20 px apart, both inside a 24 px reach.
        val word = TextSelection(0, listOf(TextRect(100.0, 700.0, 20.0, 12.0)), "hi", 0, 2)
        val spots = selectionHandleSpots(word, placement, 10.0)

        assertEquals(SelectionHandle.End, handleAt(spots, 115.0, 102.0, reachPx = 24.0))
        assertEquals(SelectionHandle.Start, handleAt(spots, 104.0, 102.0, reachPx = 24.0))
    }
}
