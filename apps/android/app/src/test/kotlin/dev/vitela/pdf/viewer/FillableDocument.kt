package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.FieldTextStyle
import dev.vitela.pdf.core.FormField
import dev.vitela.pdf.core.FormFieldKind
import dev.vitela.pdf.core.FormFieldValue
import dev.vitela.pdf.core.NewFormField
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit

/**
 * A two-page document with a text field and a checkbox on page 0 and a
 * dropdown on page 1. Every fill is undoable, like the core's.
 *
 * Rendering draws the *preview*, like the core's `render_page`: the field
 * values as they stood at open and again only after [refreshPreview].
 * [drawn] records, per render, the page and the values it showed.
 *
 * A placed field joins the list under the next id, and a placement, a move or
 * a resize is undoable like a fill; [authoringAllowed] is the stronger permission those
 * two need, [authoringRefusal] the core refusing one anyway.
 */
internal class FillableDocument(
    private val fillAllowed: Boolean = true,
    private val refusal: PdfCoreError? = null,
    private val authoringAllowed: Boolean = true,
    private val authoringRefusal: PdfCoreError? = null,
) : PdfDocument {
    private var fields = listOf(
        FormField(1, 0, "Name", FormFieldKind.Text(multiline = false, maxLength = 20), FormFieldValue.Text("Ada"), AnnotationRect(10.0, 150.0, 60.0, 20.0)),
        FormField(2, 0, "Agree", FormFieldKind.Checkbox, FormFieldValue.Checked(false), AnnotationRect(10.0, 120.0, 12.0, 12.0)),
        FormField(3, 1, "Country", FormFieldKind.Dropdown(listOf("AR", "UY"), editable = false), FormFieldValue.Choice(null), AnnotationRect(10.0, 150.0, 60.0, 20.0)),
    )
    private val undoable = ArrayDeque<Pair<List<FormField>, List<FormField>>>()
    private val redoable = ArrayDeque<Pair<List<FormField>, List<FormField>>>()
    val fills = mutableListOf<Pair<Long, FormFieldValue>>()
    val placements = mutableListOf<Triple<Int, NewFormField, AnnotationRect>>()
    val moves = mutableListOf<Pair<Long, AnnotationRect>>()
    val resizes = mutableListOf<Pair<Long, AnnotationRect>>()
    val removals = mutableListOf<Long>()
    val renames = mutableListOf<Pair<Long, String>>()
    val restyles = mutableListOf<Pair<Long, FieldTextStyle>>()
    val drawn = mutableListOf<Pair<Int, Map<Long, FormFieldValue>>>()
    private var preview = values()
    var previewRefreshes = 0
        private set

    override val pageCount: Int = 2
    override val pageSizes: List<PageSize> = List(2) { PageSize(100.0, 200.0) }

    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> {
        drawn += pageIndex to preview
        return PdfCoreResult.Success(RenderedPage(0, 0, 0, ByteArray(0)))
    }

    override fun refreshPreview(): PdfCoreResult<Unit> {
        previewRefreshes++
        preview = values()
        return PdfCoreResult.Success(Unit)
    }

    override fun search(query: String) = PdfCoreResult.Success(listOf(SearchHit(0, "x")))

    override fun annotations() = PdfCoreResult.Success(AnnotationSnapshot(emptyList(), true, undoable.isNotEmpty(), redoable.isNotEmpty()))

    override fun formFields(): PdfCoreResult<List<FormField>> = PdfCoreResult.Success(fields)

    override fun formFillAllowed(): Boolean = fillAllowed

    override fun setFormFieldValue(fieldId: Long, value: FormFieldValue): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        fills += fieldId to value
        val before = fields
        fields = fields.map { if (it.id == fieldId) it.copy(value = value) else it }
        undoable.addLast(before to fields)
        redoable.clear()
        return PdfCoreResult.Success(Unit)
    }

    override fun formAuthoringAllowed(): Boolean = authoringAllowed

    override fun addFormField(pageIndex: Int, kind: NewFormField, rect: AnnotationRect): PdfCoreResult<Unit> {
        authoringRefusal?.let { return PdfCoreResult.Failure(it) }
        placements += Triple(pageIndex, kind, rect)
        val id = fields.maxOf { it.id } + 1
        val (fieldKind, value) = when (kind) {
            NewFormField.Text -> FormFieldKind.Text(multiline = false, maxLength = null) to FormFieldValue.Text("")
            NewFormField.Checkbox -> FormFieldKind.Checkbox to FormFieldValue.Checked(false)
            NewFormField.RadioGroup -> FormFieldKind.RadioGroup(listOf("Option 1", "Option 2")) to FormFieldValue.Choice(null)
            NewFormField.Dropdown -> FormFieldKind.Dropdown(listOf("Option 1", "Option 2"), editable = false) to FormFieldValue.Choice(null)
        }
        record(fields + FormField(id, pageIndex, "${kind.name}$id", fieldKind, value, rect))
        return PdfCoreResult.Success(Unit)
    }

    override fun moveFormField(fieldId: Long, to: AnnotationRect): PdfCoreResult<Unit> {
        authoringRefusal?.let { return PdfCoreResult.Failure(it) }
        moves += fieldId to to
        record(fields.map { if (it.id == fieldId) it.copy(rect = to) else it })
        return PdfCoreResult.Success(Unit)
    }

    override fun resizeFormField(fieldId: Long, to: AnnotationRect): PdfCoreResult<Unit> {
        authoringRefusal?.let { return PdfCoreResult.Failure(it) }
        resizes += fieldId to to
        record(fields.map { if (it.id == fieldId) it.copy(rect = to) else it })
        return PdfCoreResult.Success(Unit)
    }

    override fun removeFormField(fieldId: Long): PdfCoreResult<Unit> {
        authoringRefusal?.let { return PdfCoreResult.Failure(it) }
        removals += fieldId
        record(fields.filterNot { it.id == fieldId })
        return PdfCoreResult.Success(Unit)
    }

    override fun renameFormField(fieldId: Long, name: String): PdfCoreResult<Unit> {
        authoringRefusal?.let { return PdfCoreResult.Failure(it) }
        renames += fieldId to name
        record(fields.map { if (it.id == fieldId) it.copy(name = name) else it })
        return PdfCoreResult.Success(Unit)
    }

    override fun restyleFormField(fieldId: Long, style: FieldTextStyle): PdfCoreResult<Unit> {
        authoringRefusal?.let { return PdfCoreResult.Failure(it) }
        restyles += fieldId to style
        record(fields.map { if (it.id == fieldId) it.copy(style = style) else it })
        return PdfCoreResult.Success(Unit)
    }

    private fun record(after: List<FormField>) {
        undoable.addLast(fields to after)
        redoable.clear()
        fields = after
    }

    override fun undoAnnotations(): PdfCoreResult<Boolean> {
        val step = undoable.removeLastOrNull() ?: return PdfCoreResult.Success(false)
        fields = step.first
        redoable.addLast(step)
        return PdfCoreResult.Success(true)
    }

    override fun redoAnnotations(): PdfCoreResult<Boolean> {
        val step = redoable.removeLastOrNull() ?: return PdfCoreResult.Success(false)
        fields = step.second
        undoable.addLast(step)
        return PdfCoreResult.Success(true)
    }

    override fun close() = Unit

    private fun values() = fields.associate { it.id to it.value }
}
