package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.FormField
import dev.vitela.pdf.core.FormFieldKind
import dev.vitela.pdf.core.FormFieldValue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class FormFieldsTest {
    private val field = FormField(1, 0, "Name", FormFieldKind.Checkbox, FormFieldValue.Checked(false))

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
}
