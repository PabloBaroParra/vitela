package dev.vitela.pdf.core

/**
 * One AcroForm field of the document, as the Form fields panel sees it.
 * [pageIndex] is the page's zero-based position in the current order; [rect]
 * is where the widget sits on it, in PDF points.
 */
data class FormField(
    val id: Long,
    val pageIndex: Int,
    val name: String,
    val kind: FormFieldKind,
    val value: FormFieldValue,
    val rect: AnnotationRect,
)

/** A field the panel can place. The core names it and gives it its first options and style. */
enum class NewFormField { Text, Checkbox, RadioGroup, Dropdown }

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
