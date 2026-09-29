package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.TextRect
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The drag session between a long-press and the finger lifting. The page's
 * characters load off the main thread, so the finger is already moving before
 * the session can answer — nothing it does in that gap may be lost.
 */
class TextSelectionDragTest {
    @Test
    fun theSelectionRunsFromTheAnchorToTheFocusAsTheCoreReportsIt() {
        val characters = LineCharacters("Hello")
        val drag = TextSelectionDrag(3, AnnotationPoint(10.0, 705.0))
        drag.attach(characters)

        drag.extend(AnnotationPoint(40.0, 705.0))

        assertEquals(TextSelection(3, listOf(TextRect(10.0, 700.0, 30.0, 12.0)), "ell"), drag.selection())
    }

    @Test
    fun aDragBackwardsSelectsTheSameText() {
        val drag = TextSelectionDrag(0, AnnotationPoint(40.0, 705.0))
        drag.attach(LineCharacters("Hello"))

        drag.extend(AnnotationPoint(10.0, 705.0))

        assertEquals("ell", drag.selection()?.text)
    }

    @Test
    fun aDragThatHasNotMovedSelectsNothing() {
        val drag = TextSelectionDrag(0, AnnotationPoint(20.0, 705.0))
        drag.attach(LineCharacters("Hello"))

        assertNull(drag.selection())
    }

    @Test
    fun movesMadeBeforeTheCharactersLoadedAreAppliedOnceTheyDo() {
        val drag = TextSelectionDrag(0, AnnotationPoint(0.0, 705.0))
        drag.extend(AnnotationPoint(20.0, 705.0))
        drag.extend(AnnotationPoint(50.0, 705.0))
        assertFalse(drag.isLoaded)
        assertNull(drag.selection())

        drag.attach(LineCharacters("Hello"))

        assertEquals("Hello", drag.selection()?.text)
    }

    @Test
    fun aPageWithoutPositionedTextSelectsNothing() {
        val drag = TextSelectionDrag(0, AnnotationPoint(10.0, 705.0))
        drag.attach(LineCharacters(""))
        drag.extend(AnnotationPoint(40.0, 705.0))

        assertNull(drag.selection())
    }

    @Test
    fun closingReleasesTheNativeCharacters() {
        val characters = LineCharacters("Hello")
        val drag = TextSelectionDrag(0, AnnotationPoint(10.0, 705.0))
        drag.attach(characters)

        drag.close()

        assertTrue(characters.closed)
        assertNull(drag.selection())
    }

    @Test
    fun charactersThatArriveAfterTheSessionClosedAreReleasedAtOnce() {
        // The user replaced the document, or started another drag, while this
        // page's characters were still loading. Nobody will ever close them.
        val drag = TextSelectionDrag(0, AnnotationPoint(10.0, 705.0))
        drag.close()
        val late = LineCharacters("Hello")

        drag.attach(late)

        assertTrue(late.closed)
        assertFalse(drag.isLoaded)
    }
}

/**
 * One line of text at y 700..712, each character 10 pt wide from x = 0. Its
 * caret rule is deliberately trivial: the core's real line-then-column logic
 * is tested in `pdf_render::selection`, not re-tested here.
 */
internal class LineCharacters(private val text: String) : dev.vitela.pdf.core.PageCharacters {
    var closed = false
        private set

    override fun caretAt(point: AnnotationPoint): Int? =
        if (text.isEmpty()) null else Math.round(point.x / 10.0).toInt().coerceIn(0, text.length)

    override fun textIn(anchor: Int, focus: Int): String = text.substring(minOf(anchor, focus), maxOf(anchor, focus))

    override fun rectsIn(anchor: Int, focus: Int): List<TextRect> {
        val start = minOf(anchor, focus)
        val end = maxOf(anchor, focus)
        return if (start == end) emptyList() else listOf(TextRect(start * 10.0, 700.0, (end - start) * 10.0, 12.0))
    }

    override fun close() {
        closed = true
    }
}
