package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.FormField
import dev.vitela.pdf.core.FormFieldKind
import dev.vitela.pdf.core.FormFieldValue

/**
 * The open Form fields panel: one row per field the document already has,
 * built from the core's list and never from the page.
 *
 * [fields] carries each value as the panel last showed or committed it, so a
 * row left untouched — or committed twice, on Done and again on losing
 * focus — never costs a round trip to the core.
 */
data class FormFieldsState(
    val fields: List<FormField> = emptyList(),
    val fillAllowed: Boolean = false,
    /** False until the core's list arrives, so an empty form is not announced before it is known to be empty. */
    val loaded: Boolean = false,
)

// Wording is the Windows shell's.
internal const val FORM_NO_CHOICE = "(none)"
internal const val FORM_FILLED = "Field filled in. Save to keep the change."
internal const val FORM_NO_FIELDS = "This document has no form fields."
internal const val FORM_FILL_FORBIDDEN = "This document does not permit filling in its form."

/** The line the panel shows above its rows, or null when the rows speak for themselves. */
internal fun formFieldsNotice(panel: FormFieldsState): String? = when {
    !panel.loaded -> null
    panel.fields.isEmpty() -> FORM_NO_FIELDS
    !panel.fillAllowed -> FORM_FILL_FORBIDDEN
    else -> null
}

/**
 * What a dropdown shows for [choice]. A value outside [options] — possible in
 * a file another tool wrote — shows as no choice rather than as an entry the
 * menu does not have.
 */
internal fun choiceLabel(options: List<String>, choice: String?): String =
    choice?.takeIf { it in options } ?: FORM_NO_CHOICE

/** An editable dropdown is typed into; clearing it chooses nothing. */
internal fun editableChoice(text: String): FormFieldValue.Choice = FormFieldValue.Choice(text.ifEmpty { null })

/** [text] cut to the field's limit, so the row cannot offer a value the core would refuse. */
internal fun cappedText(text: String, maxLength: Int?): String = if (maxLength == null) text else text.take(maxLength)

/** The core reports a kind it does not model as a text field with a zero limit: nobody may type into it. */
internal val FormFieldKind.Text.readOnly: Boolean get() = maxLength == 0

internal fun List<FormField>.withValue(fieldId: Long, value: FormFieldValue): List<FormField> =
    map { if (it.id == fieldId) it.copy(value = value) else it }
