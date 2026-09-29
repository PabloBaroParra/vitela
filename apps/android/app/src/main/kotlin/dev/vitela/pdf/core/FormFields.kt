package dev.vitela.pdf.core

/**
 * One AcroForm field the document already has, as the fill panel sees it.
 * [pageIndex] is the page's zero-based position in the current order.
 */
data class FormField(
    val id: Long,
    val pageIndex: Int,
    val name: String,
    val kind: FormFieldKind,
    val value: FormFieldValue,
)

/** What a field can hold. Pushbuttons, listboxes and signatures never reach the shell. */
sealed interface FormFieldKind {
    /**
     * [maxLength] null means no limit. Zero means nobody may type into it: the
     * core reports a kind it does not model that way.
     */
    data class Text(val multiline: Boolean, val maxLength: Int?) : FormFieldKind
    data object Checkbox : FormFieldKind
    /** [options] are the export values, one per button. */
    data class RadioGroup(val options: List<String>) : FormFieldKind
    /** An [editable] dropdown also accepts text outside [options]. */
    data class Dropdown(val options: List<String>, val editable: Boolean) : FormFieldKind
}

/** What a field holds. Which variant fits depends on its [FormFieldKind]; the core validates it. */
sealed interface FormFieldValue {
    data class Text(val text: String) : FormFieldValue
    data class Checked(val checked: Boolean) : FormFieldValue
    /** [option] null means nothing is chosen yet. */
    data class Choice(val option: String?) : FormFieldValue
}
