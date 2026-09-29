package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.FormFieldValue
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Form fields: filling in the AcroForm fields a document already has. Each
 * fill is one undoable entry in the shared edit log, so Undo and Redo keep
 * working from the reader chrome.
 *
 * Nothing is drawn over the page: pdfium paints a field's value from the
 * appearance the core regenerates, so a fill rebuilds the preview and the
 * page re-renders showing it — the Windows shell's approach.
 */
internal class FormFilling(
    private val session: ViewerSession,
    private val annotations: AnnotationEditing,
    private val layout: PageLayout,
) {
    private val state = session.state

    /** Opens the panel below the reader. Not over the Organize grid, which hides the pages a fill would redraw. */
    fun open() {
        val openDocument = session.document ?: return
        if (state.value.pageCount == 0 || state.value.organize != null || state.value.formFields != null) return
        state.value = state.value.copy(formFields = FormFieldsState())
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document === openDocument) reread(openDocument)
            }
        }
    }

    fun close() {
        if (state.value.formFields != null) state.value = state.value.copy(formFields = null)
    }

    /**
     * Fills field [fieldId] with [value], for the document [documentId] its row
     * was built for. A row can still commit after another document replaced
     * it — a text field losing focus as the panel goes away — and its field id
     * means nothing against the new file, so that commit is dropped.
     */
    fun fill(documentId: Long, fieldId: Long, value: FormFieldValue) {
        val openDocument = session.document ?: return
        val panel = state.value.formFields ?: return
        if (documentId != state.value.documentId || !panel.fillAllowed) return
        val field = panel.fields.firstOrNull { it.id == fieldId } ?: return
        if (field.value == value) return
        // Shown before the core confirms it, so the same value arriving again
        // (Done, then the focus loss) is recognised as already committed.
        state.value = state.value.copy(formFields = panel.copy(fields = panel.fields.withValue(fieldId, value)))
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                when (val result = withContext(session.compute) { openDocument.setFormFieldValue(fieldId, value) }) {
                    is PdfCoreResult.Success -> {
                        layout.markRedrawn()
                        state.value = state.value.copy(isDirty = true, revision = state.value.revision + 1)
                        // The fill sits in the shared log: Undo must light up.
                        annotations.refresh(openDocument)
                        if (layout.redraw(openDocument)) state.value = state.value.copy(status = FORM_FILLED)
                    }
                    is PdfCoreResult.Failure -> {
                        // Put the rows back to what the core holds, then say why.
                        reread(openDocument)
                        state.value = state.value.copy(status = userMessage(result.error))
                    }
                }
            }
        }
    }

    /**
     * Re-reads the fields into an open panel: at open, after a refused fill,
     * and after an undo or redo, neither of which says which field it moved.
     * Called with the document lane held.
     */
    suspend fun reread(document: PdfDocument) {
        if (state.value.formFields == null) return
        val (result, allowed) = withContext(session.compute) { document.formFields() to document.formFillAllowed() }
        // Closed, or replaced by another document's, while the core answered.
        if (state.value.formFields == null || session.document !== document) return
        when (result) {
            is PdfCoreResult.Success -> state.value = state.value.copy(formFields = FormFieldsState(result.value, allowed, loaded = true))
            // Not loaded: "no form fields" would be a claim the core never made.
            is PdfCoreResult.Failure -> state.value = state.value.copy(
                formFields = FormFieldsState(fillAllowed = allowed),
                status = userMessage(result.error),
            )
        }
    }
}
