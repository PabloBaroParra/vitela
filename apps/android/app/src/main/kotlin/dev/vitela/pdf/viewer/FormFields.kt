package dev.vitela.pdf.viewer

import dev.vitela.pdf.R
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.FormField
import dev.vitela.pdf.core.FormFieldKind
import dev.vitela.pdf.core.FormFieldValue
import dev.vitela.pdf.core.NewFormField
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.text.UiText

/**
 * The open Form fields panel: one row per field of the document, built from
 * the core's list and never from the page.
 *
 * [fields] carries each value as the panel last showed or committed it, so a
 * row left untouched — or committed twice, on Done and again on losing
 * focus — never costs a round trip to the core.
 */
data class FormFieldsState(
    val fields: List<FormField> = emptyList(),
    val fillAllowed: Boolean = false,
    /** Whether fields may be placed and moved: the stronger permission, not [fillAllowed]. */
    val authoringAllowed: Boolean = false,
    /** What the next page tap does, or null while page taps belong to the reader. */
    val armed: FormFieldTap? = null,
    /** False until the core's list arrives, so an empty form is not announced before it is known to be empty. */
    val loaded: Boolean = false,
)

/** A page tap the panel has armed: placing a new field, or moving one of its rows' fields. */
sealed interface FormFieldTap {
    data class Place(val kind: NewFormField) : FormFieldTap
    /** [pageIndex] is the field's page: a move keeps a field on it. */
    data class Move(val fieldId: Long, val pageIndex: Int) : FormFieldTap
}

// Wording is the Windows shell's.
internal const val FORM_NO_CHOICE = "(none)"
internal val FORM_FILLED: UiText = UiText.Resource(R.string.status_form_filled)
internal val FORM_NO_FIELDS: UiText = UiText.Resource(R.string.status_form_none)
internal val FORM_NO_FIELDS_ADD: UiText = UiText.Resource(R.string.status_form_none_add)
internal val FORM_FILL_FORBIDDEN: UiText = UiText.Resource(R.string.status_form_fill_forbidden)
internal val FIELD_MOVED: UiText = UiText.Resource(R.string.status_form_moved)
internal val FIELD_RESIZED: UiText = UiText.Resource(R.string.status_form_resized)
internal val FIELD_DELETED: UiText = UiText.Resource(R.string.status_form_deleted)
internal val FIELD_SIZE_INVALID: UiText = UiText.Resource(R.string.status_form_size_invalid)
internal val FIELD_RENAMED: UiText = UiText.Resource(R.string.status_form_renamed)
internal val FIELD_RESTYLED: UiText = UiText.Resource(R.string.status_form_restyled)
internal val FIELD_NAME_EMPTY: UiText = UiText.Resource(R.string.status_form_name_empty)
internal val FIELD_FONT_SIZE_INVALID: UiText = UiText.Resource(R.string.status_form_font_size_invalid)

/** The font sizes a field may be given, in points: the Windows shell's bounds. */
internal val FIELD_FONT_SIZES = 1.0..72.0

internal fun fieldPlacementPrompt(kind: NewFormField): UiText = UiText.Resource(
    when (kind) {
        NewFormField.Text -> R.string.status_form_place_text
        NewFormField.Checkbox -> R.string.status_form_place_checkbox
        NewFormField.RadioGroup -> R.string.status_form_place_radio_group
        NewFormField.Dropdown -> R.string.status_form_place_dropdown
    },
)

internal fun fieldPlacedStatus(kind: NewFormField): UiText = UiText.Resource(
    when (kind) {
        NewFormField.Text -> R.string.status_form_placed_text
        NewFormField.Checkbox -> R.string.status_form_placed_checkbox
        NewFormField.RadioGroup -> R.string.status_form_placed_radio_group
        NewFormField.Dropdown -> R.string.status_form_placed_dropdown
    },
)

internal fun fieldWrongPage(field: FormField): UiText = UiText.Resource(R.string.status_form_wrong_page, field.pageIndex + 1, field.name)
internal fun fieldNameTaken(name: String): UiText = UiText.Resource(R.string.status_form_name_taken, name)

/** The line the panel shows above its rows, or null when the rows speak for themselves. */
internal fun formFieldsNotice(panel: FormFieldsState): UiText? = when (val armed = panel.armed) {
    is FormFieldTap.Place -> fieldPlacementPrompt(armed.kind)
    is FormFieldTap.Move -> panel.fields.firstOrNull { it.id == armed.fieldId }?.let { UiText.Resource(R.string.status_form_move_prompt, it.pageIndex + 1, it.name) }
    null -> when {
        !panel.loaded -> null
        panel.fields.isEmpty() -> if (panel.authoringAllowed) FORM_NO_FIELDS_ADD else FORM_NO_FIELDS
        !panel.fillAllowed -> FORM_FILL_FORBIDDEN
        else -> null
    }
}

/**
 * Where a tap at [tap] places a new field of [kind] on a page of [page] size:
 * the Windows shell's click size, hanging below and right of the tap — PDF
 * space grows upward — and kept whole on the page. A tap, never a drag: on a
 * phone a drag is the reader's scroll.
 */
internal fun placedFieldRect(kind: NewFormField, tap: AnnotationPoint, page: PageSize): AnnotationRect {
    val (width, height) = when (kind) {
        NewFormField.Checkbox -> 18.0 to 18.0
        NewFormField.RadioGroup -> 144.0 to 48.0
        NewFormField.Text, NewFormField.Dropdown -> 144.0 to 36.0
    }
    val unrotated = page.unrotated
    return cornerAt(tap, minOf(width, unrotated.widthPt), minOf(height, unrotated.heightPt), page)
}

/** Where a tap at [tap] moves a field now at [rect]: its top-left corner to the tap, its size kept, still on the page. */
internal fun movedFieldRect(rect: AnnotationRect, tap: AnnotationPoint, page: PageSize): AnnotationRect =
    cornerAt(tap, rect.width, rect.height, page)

/**
 * A field now at [rect] at [width] by [height] points, or null for a size that
 * is not a finite, positive number. Its top-left corner stays put — the corner
 * a move lands on, and where the reader's eye is — so it grows down and right,
 * and it is kept whole on the page.
 */
internal fun resizedFieldRect(rect: AnnotationRect, width: Double, height: Double, page: PageSize): AnnotationRect? {
    if (!width.isFinite() || !height.isFinite() || width <= 0.0 || height <= 0.0) return null
    val unrotated = page.unrotated
    return cornerAt(AnnotationPoint(rect.x, rect.y + rect.height), minOf(width, unrotated.widthPt), minOf(height, unrotated.heightPt), page)
}

/** A size in points as the row shows it: at most two decimals, none when whole. */
internal fun pointsText(points: Double): String =
    java.math.BigDecimal.valueOf(points).setScale(2, java.math.RoundingMode.HALF_UP).stripTrailingZeros().toPlainString()

/** What the reader typed, as points; NaN for anything that is not a number, so the edit is refused and says why. A comma is a decimal point. */
internal fun typedPoints(text: String): Double = text.trim().replace(',', '.').toDoubleOrNull() ?: Double.NaN

/** Kept whole on the page in the unrotated space the field's rect lives in, not the turned size it is drawn at. */
private fun cornerAt(tap: AnnotationPoint, width: Double, height: Double, page: PageSize) = page.unrotated.let { unrotated ->
    AnnotationRect(
        tap.x.coerceIn(0.0, (unrotated.widthPt - width).coerceAtLeast(0.0)),
        (tap.y - height).coerceIn(0.0, (unrotated.heightPt - height).coerceAtLeast(0.0)),
        width,
        height,
    )
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
