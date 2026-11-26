package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentFontKind
import dev.vitela.pdf.core.ContentTextRun
import dev.vitela.pdf.core.TextRunLook
import dev.vitela.pdf.core.TextRunLookState
import dev.vitela.pdf.core.TextStyleToggle
import dev.vitela.pdf.text.english
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The toggle rule, as the Windows shell's TextStylePolicy has it: the buttons follow the first run. */
class TextStyleTest {
    private val hello = ContentTextRun(0, 0, AnnotationRect(72.0, 700.0, 30.0, 12.0), "F1", ContentFontKind.Standard14, "Hello")
    private val world = ContentTextRun(1, 0, AnnotationRect(102.0, 700.0, 32.0, 12.0), "F2", ContentFontKind.Standard14, "world")
    private val plain = TextRunLookState(TextRunLook())
    private val bold = TextRunLook(bold = true)
    private val fixedBold = TextRunLookState(bold, canRemoveBold = false)

    @Test
    fun aPlainFirstRunCanAlwaysBeMadeBold() {
        val buttons = textStyleButtons(listOf(plain, fixedBold))!!
        assertEquals(TextRunLook(), buttons.look)
        assertTrue(buttons.boldRemovable)
        assertFalse(buttons.isFixed(TextStyleToggle.Bold))
    }

    @Test
    fun aNativeBoldWithNoRegularSiblingShowsPressedAndFixed() {
        val buttons = textStyleButtons(listOf(fixedBold, plain))!!
        assertTrue(buttons.look.bold)
        assertTrue(buttons.isFixed(TextStyleToggle.Bold))
        assertFalse(buttons.isFixed(TextStyleToggle.Italic))
        assertFalse("underline is never a font's own", buttons.isFixed(TextStyleToggle.Underline))
    }

    @Test
    fun noTargetShowsNoButtonsAndRecordsNothing() {
        assertNull(textStyleButtons(emptyList()))
        val press = textStylePress(emptyList(), TextStyleToggle.Italic)
        assertTrue(press.restyles.isEmpty())
        assertNull(press.refusal)
    }

    @Test
    fun aPressEvensAMixedTargetOut() {
        val press = textStylePress(listOf(hello to plain, world to TextRunLookState(bold)), TextStyleToggle.Bold)
        assertEquals(listOf(hello), press.restyles.map { it.run })
        assertEquals(TextRunLook(), press.restyles.single().from)
        assertEquals(bold, press.restyles.single().to)
    }

    @Test
    fun aPressOnAStyledFirstRunTakesItOffEveryRunThatHasIt() {
        val underlined = TextRunLookState(TextRunLook(underline = true, strike = true))
        val press = textStylePress(listOf(hello to underlined, world to plain), TextStyleToggle.Underline)
        assertEquals(listOf(hello), press.restyles.map { it.run })
        assertEquals("strike is carried through", TextRunLook(strike = true), press.restyles.single().to)
    }

    @Test
    fun aFixedBoldIsRefusedWithItsReason() {
        val press = textStylePress(listOf(hello to fixedBold), TextStyleToggle.Bold)
        assertTrue(press.restyles.isEmpty())
        assertEquals(fixedStyleReason(TextStyleToggle.Bold), press.refusal)
        assertTrue(press.refusal!!.english().contains("regular"))
    }

    @Test
    fun italicStillGoesOnOverAFixedBold() {
        val press = textStylePress(listOf(hello to fixedBold, world to TextRunLookState(bold)), TextStyleToggle.Italic)
        assertNull(press.refusal)
        assertEquals(listOf(hello, world), press.restyles.map { it.run })
        assertTrue(press.restyles.all { it.to == TextRunLook(bold = true, italic = true) })
    }

    @Test
    fun aFixedItalicNamesItalic() {
        val press = textStylePress(listOf(hello to TextRunLookState(TextRunLook(italic = true), canRemoveItalic = false)), TextStyleToggle.Italic)
        assertEquals(fixedStyleReason(TextStyleToggle.Italic), press.refusal)
        assertTrue(press.refusal!!.english().contains("upright"))
    }
}
