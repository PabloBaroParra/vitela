package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.FormField
import dev.vitela.pdf.core.FormFieldKind
import dev.vitela.pdf.core.FormFieldValue
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
 */
internal class FillableDocument(
    private val fillAllowed: Boolean = true,
    private val refusal: PdfCoreError? = null,
) : PdfDocument {
    private var fields = listOf(
        FormField(1, 0, "Name", FormFieldKind.Text(multiline = false, maxLength = 20), FormFieldValue.Text("Ada")),
        FormField(2, 0, "Agree", FormFieldKind.Checkbox, FormFieldValue.Checked(false)),
        FormField(3, 1, "Country", FormFieldKind.Dropdown(listOf("AR", "UY"), editable = false), FormFieldValue.Choice(null)),
    )
    private val undoable = ArrayDeque<Pair<List<FormField>, List<FormField>>>()
    private val redoable = ArrayDeque<Pair<List<FormField>, List<FormField>>>()
    val fills = mutableListOf<Pair<Long, FormFieldValue>>()
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
