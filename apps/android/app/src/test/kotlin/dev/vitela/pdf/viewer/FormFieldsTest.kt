package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.FormField
import dev.vitela.pdf.core.FormFieldKind
import dev.vitela.pdf.core.FormFieldValue
import dev.vitela.pdf.core.NewFormField
import dev.vitela.pdf.core.PageSize
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class FormFieldsTest {
    private val field = FormField(1, 0, "Name", FormFieldKind.Checkbox, FormFieldValue.Checked(false), AnnotationRect(10.0, 20.0, 18.0, 18.0))

    @Test
    fun theNoticeWaitsForTheCoresList() {
        assertNull(formFieldsNotice(FormFieldsState()))
    }

    @Test
    fun aDocumentWithoutFieldsSaysSo() {
        assertEquals(FORM_NO_FIELDS, formFieldsNotice(FormFieldsState(loaded = true, fillAllowed = true)))
    }

    @Test
    fun aDocumentThatForbidsFillingSaysSo() {
        assertEquals(FORM_FILL_FORBIDDEN, formFieldsNotice(FormFieldsState(listOf(field), fillAllowed = false, loaded = true)))
    }

    @Test
    fun aFillableFormNeedsNoNotice() {
        assertNull(formFieldsNotice(FormFieldsState(listOf(field), fillAllowed = true, loaded = true)))
    }

    @Test
    fun aChoiceOutsideTheOptionsShowsAsNoChoice() {
        assertEquals("UY", choiceLabel(listOf("AR", "UY"), "UY"))
        assertEquals(FORM_NO_CHOICE, choiceLabel(listOf("AR", "UY"), null))
        // Another tool may have written a value the options do not list.
        assertEquals(FORM_NO_CHOICE, choiceLabel(listOf("AR", "UY"), "CL"))
    }

    @Test
    fun clearingAnEditableDropdownChoosesNothing() {
        assertEquals(FormFieldValue.Choice(null), editableChoice(""))
        assertEquals(FormFieldValue.Choice("Other"), editableChoice("Other"))
    }

    @Test
    fun typedTextIsCappedAtTheFieldsLimit() {
        assertEquals("abc", cappedText("abcdef", 3))
        assertEquals("abcdef", cappedText("abcdef", null))
    }

    @Test
    fun aZeroLimitMeansNobodyMayType() {
        assertEquals(true, FormFieldKind.Text(multiline = false, maxLength = 0).readOnly)
        assertEquals(false, FormFieldKind.Text(multiline = false, maxLength = null).readOnly)
    }

    @Test
    fun aValueReplacesOnlyItsOwnField() {
        val other = field.copy(id = 2)
        val updated = listOf(field, other).withValue(2, FormFieldValue.Checked(true))
        assertEquals(listOf(field, other.copy(value = FormFieldValue.Checked(true))), updated)
    }

    private val letter = PageSize(612.0, 792.0)

    @Test
    fun aPlacedFieldHangsBelowAndRightOfTheTap() {
        // PDF space grows upward, so the box's bottom edge is the tap minus its height.
        assertEquals(AnnotationRect(100.0, 664.0, 144.0, 36.0), placedFieldRect(NewFormField.Text, AnnotationPoint(100.0, 700.0), letter))
        assertEquals(AnnotationRect(100.0, 682.0, 18.0, 18.0), placedFieldRect(NewFormField.Checkbox, AnnotationPoint(100.0, 700.0), letter))
        assertEquals(AnnotationRect(100.0, 652.0, 144.0, 48.0), placedFieldRect(NewFormField.RadioGroup, AnnotationPoint(100.0, 700.0), letter))
        assertEquals(AnnotationRect(100.0, 664.0, 144.0, 36.0), placedFieldRect(NewFormField.Dropdown, AnnotationPoint(100.0, 700.0), letter))
    }

    @Test
    fun aPlacedFieldNearAnEdgeStaysOnThePage() {
        assertEquals(AnnotationRect(468.0, 0.0, 144.0, 36.0), placedFieldRect(NewFormField.Text, AnnotationPoint(600.0, 10.0), letter))
    }

    @Test
    fun aFieldTooBigForThePageShrinksToIt() {
        assertEquals(AnnotationRect(0.0, 0.0, 100.0, 30.0), placedFieldRect(NewFormField.Text, AnnotationPoint(50.0, 20.0), PageSize(100.0, 30.0)))
    }

    @Test
    fun aMovedFieldKeepsItsSizeAndPutsItsCornerAtTheTap() {
        val rect = AnnotationRect(10.0, 20.0, 144.0, 36.0)
        assertEquals(AnnotationRect(200.0, 364.0, 144.0, 36.0), movedFieldRect(rect, AnnotationPoint(200.0, 400.0), letter))
        assertEquals(AnnotationRect(468.0, 756.0, 144.0, 36.0), movedFieldRect(rect, AnnotationPoint(611.0, 800.0), letter))
    }

    @Test
    fun theArmedTapIsAnnouncedAboveTheRows() {
        val panel = FormFieldsState(listOf(field), fillAllowed = true, authoringAllowed = true, loaded = true)
        assertEquals("Tap a page to place a checkbox.", formFieldsNotice(panel.copy(armed = FormFieldTap.Place(NewFormField.Checkbox))))
        assertEquals("Tap page 1 where Name should go.", formFieldsNotice(panel.copy(armed = FormFieldTap.Move(1, 0))))
    }

    @Test
    fun anEmptyFormThatAcceptsNewFieldsSaysHowToAddOne() {
        assertEquals(FORM_NO_FIELDS_ADD, formFieldsNotice(FormFieldsState(loaded = true, fillAllowed = true, authoringAllowed = true)))
    }
}
