package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Form fields: placing new fields, moving, resizing and deleting existing ones, each one
 * undoable entry in the shared edit log. The panel arms a page tap for a place
 * or a move, and the tap is the edit; a size is typed into the field's row.
 *
 * A tap, never a drag: on a phone a drag is the reader's scroll, so a field is
 * placed at the Windows shell's click size and a move keeps the field's size.
 * The core names a new field and picks its style and first options.
 *
 * Nothing is drawn over the page, as with a fill: pdfium paints the field from
 * the appearance the core writes, so an edit rebuilds the preview and the page
 * re-renders showing it.
 */
internal class FormAuthoring(
    private val session: ViewerSession,
    private val annotations: AnnotationEditing,
    private val layout: PageLayout,
    private val filling: FormFilling,
    private val selection: TextSelecting,
) {
    private val state = session.state

    /**
     * Arms [tap] for the next page tap, or disarms with null. One mode claims a
     * page tap at a time, so Edit text, an armed annotation tool and a text
     * selection are dropped.
     */
    fun arm(tap: FormFieldTap?) {
        val panel = state.value.formFields ?: return
        if (tap == null) {
            if (panel.armed != null) state.value = state.value.copy(formFields = panel.copy(armed = null))
            return
        }
        if (!panel.authoringAllowed) return
        if (tap is FormFieldTap.Move && panel.fields.none { it.id == tap.fieldId }) return
        selection.clear()
        val armed = panel.copy(armed = tap)
        state.value = state.value.copy(
            formFields = armed,
            contentEdit = null,
            activeAnnotationTool = AnnotationTool.Pointer,
            selectedAnnotationId = null,
            status = formFieldsNotice(armed) ?: state.value.status,
        )
    }

    fun disarm() = arm(null)

    /** A tap at [point] on page [pageIndex] while the panel has one armed. */
    fun tap(pageIndex: Int, point: AnnotationPoint) {
        val openDocument = session.document ?: return
        val panel = state.value.formFields ?: return
        val page = state.value.pageSizes.getOrNull(pageIndex) ?: return
        when (val armed = panel.armed) {
            null -> return
            is FormFieldTap.Place -> {
                val rect = placedFieldRect(armed.kind, point, page)
                commit(openDocument, fieldPlacedStatus(armed.kind)) { addFormField(pageIndex, armed.kind, rect) }
            }
            is FormFieldTap.Move -> {
                val field = panel.fields.firstOrNull { it.id == armed.fieldId } ?: return disarm()
                // `MoveFormField` carries a rect, not a page: a field stays where it is.
                if (field.pageIndex != pageIndex) {
                    state.value = state.value.copy(status = fieldWrongPage(field))
                    return
                }
                val to = movedFieldRect(field.rect, point, page)
                if (to == field.rect) return disarm()
                commit(openDocument, FIELD_MOVED) { moveFormField(field.id, to) }
            }
        }
    }

    /**
     * Resizes field [fieldId] to [width] by [height] points, for the document
     * [documentId] its row was built for — a row can still commit on losing
     * focus after another file replaced it, and its field id means nothing there.
     */
    fun resize(documentId: Long, fieldId: Long, width: Double, height: Double) {
        val openDocument = session.document ?: return
        val panel = state.value.formFields ?: return
        if (documentId != state.value.documentId || !panel.authoringAllowed) return
        val field = panel.fields.firstOrNull { it.id == fieldId } ?: return
        val page = state.value.pageSizes.getOrNull(field.pageIndex) ?: return
        val to = resizedFieldRect(field.rect, width, height, page)
            ?: return run { state.value = state.value.copy(status = FIELD_SIZE_INVALID) }
        if (to == field.rect) return
        commit(openDocument, FIELD_RESIZED) { resizeFormField(field.id, to) }
    }

    /**
     * Deletes field [fieldId] and its widgets as one undo step, for the
     * document [documentId] its row was built for. A move armed on it goes
     * with it: [commit] disarms first.
     */
    fun remove(documentId: Long, fieldId: Long) {
        val openDocument = session.document ?: return
        val panel = state.value.formFields ?: return
        if (documentId != state.value.documentId || !panel.authoringAllowed) return
        if (panel.fields.none { it.id == fieldId }) return
        commit(openDocument, FIELD_DELETED) { removeFormField(fieldId) }
    }

    /**
     * Sends one field edit. Disarmed first — one tap, one edit — so a second
     * tap before the core answers places nothing.
     */
    private fun commit(openDocument: PdfDocument, done: String, edit: PdfDocument.() -> PdfCoreResult<Unit>) {
        disarm()
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                when (val result = withContext(session.compute) { openDocument.edit() }) {
                    is PdfCoreResult.Success -> {
                        layout.markRedrawn()
                        state.value = state.value.copy(isDirty = true, revision = state.value.revision + 1)
                        // The edit sits in the shared log: Undo must light up.
                        annotations.refresh(openDocument)
                        val redrawn = layout.redraw(openDocument)
                        // A new row, or a row whose rect changed.
                        filling.reread(openDocument)
                        if (redrawn) state.value = state.value.copy(status = done)
                    }
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }
}
