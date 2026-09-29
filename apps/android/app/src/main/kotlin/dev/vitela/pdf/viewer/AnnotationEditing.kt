package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationColor
import dev.vitela.pdf.core.AnnotationEdit
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * The annotation toolbar and its gestures, plus undo/redo of the shared edit
 * log. A text selection is an input here: the markup tools turn it into
 * annotations, and a tap elsewhere clears it.
 */
internal class AnnotationEditing(private val session: ViewerSession, private val selection: TextSelecting) {
    private val state = session.state
    private var stampBytes: ByteArray? = null

    fun setTool(tool: AnnotationTool) {
        if (!state.value.annotationEditingAllowed) return
        val textSelection = state.value.textSelection
        if (textSelection != null && tool in setOf(AnnotationTool.Highlight, AnnotationTool.Underline, AnnotationTool.Strikeout)) {
            applyEdits(markupTextSelection(tool, textSelection).map(AnnotationEdit::Add))
            selection.closeDrag()
            state.value = state.value.copy(activeAnnotationTool = AnnotationTool.Pointer, textSelection = null)
        } else {
            state.value = state.value.copy(activeAnnotationTool = tool)
        }
    }

    fun select(pageIndex: Int, point: AnnotationPoint) {
        val selected = annotationAt(state.value.annotations, pageIndex, point)
        state.value = state.value.copy(selectedAnnotationId = selected?.id)
    }

    fun handlePageGesture(pageIndex: Int, origin: AnnotationPoint, current: AnnotationPoint, points: List<AnnotationPoint>, handleReach: Double) {
        if (state.value.activeAnnotationTool != AnnotationTool.Pointer) {
            place(pageIndex, origin, current, points)
            return
        }
        val selected = selectedAnnotation()?.takeIf { it.pageIndex == pageIndex }
        when (val mode = selected?.let { dragModeAt(it, origin, handleReach) }) {
            DragMode.Move -> moveSelected(origin, current)
            is DragMode.Resize -> resizeSelected(mode.corner, current)
            null -> {
                val hit = annotationAt(state.value.annotations, pageIndex, origin)
                // Only a tap gets here: an unclaimed pointer drag is the list's
                // scroll, and a drag-select starts from a long-press instead.
                selection.clear()
                if (hit != null) state.value = state.value.copy(selectedAnnotationId = hit.id)
            }
        }
    }

    fun place(pageIndex: Int, origin: AnnotationPoint, current: AnnotationPoint, points: List<AnnotationPoint> = emptyList()) {
        val tool = state.value.activeAnnotationTool
        if (!state.value.annotationEditingAllowed || tool == AnnotationTool.Pointer || (tool == AnnotationTool.Ink && points.size < 2)) return
        if (tool == AnnotationTool.Stamp) {
            val image = stampBytes ?: run {
                state.value = state.value.copy(status = "Choose an image before placing a stamp.")
                return
            }
            insertImageStamp(pageIndex, image, origin)
        } else {
            applyEdit(AnnotationEdit.Add(placementAnnotation(tool, pageIndex, origin, current, points)))
        }
        state.value = state.value.copy(activeAnnotationTool = AnnotationTool.Pointer)
    }

    fun selectImageStamp(bytes: ByteArray) {
        if (!state.value.annotationEditingAllowed) return
        stampBytes = bytes
        state.value = state.value.copy(activeAnnotationTool = AnnotationTool.Stamp, status = "Tap a page to place the image stamp.")
    }

    fun moveSelected(origin: AnnotationPoint, current: AnnotationPoint) {
        val selected = selectedAnnotation() ?: return
        if (origin == current) return
        applyEdit(AnnotationEdit.Move(selected.id, current.x - origin.x, current.y - origin.y))
    }

    fun resizeSelected(corner: HandleCorner, point: AnnotationPoint) {
        val selected = selectedAnnotation() ?: return
        val rect = selected.rect ?: return
        applyEdit(AnnotationEdit.Resize(selected.id, resizedRect(rect, corner, point)))
    }

    fun growSelected() {
        val selected = selectedAnnotation() ?: return
        selected.rect?.let { applyEdit(AnnotationEdit.Resize(selected.id, grownRect(it))) }
    }

    fun restyleSelected(color: AnnotationColor) {
        selectedAnnotation()?.takeIf { it.supportsRestyle }?.let { applyEdit(AnnotationEdit.Restyle(it.id, color)) }
    }

    fun deleteSelected() {
        selectedAnnotation()?.let { annotation ->
            applyEdit(AnnotationEdit.Remove(annotation.id))
            state.value = state.value.copy(selectedAnnotationId = null)
        }
    }

    fun undo() = applyHistory(undo = true)
    fun redo() = applyHistory(undo = false)

    /**
     * Re-reads the annotations and the edit log's undo/redo availability.
     * Every change to the shared log calls this, annotation or not: Undo must
     * light up after any queued edit.
     */
    suspend fun refresh(openDocument: PdfDocument) {
        when (val result = withContext(Dispatchers.Default) { openDocument.annotations() }) {
            is PdfCoreResult.Success -> state.value = state.value.copy(
                annotations = result.value.annotations,
                annotationEditingAllowed = result.value.editingAllowed,
                canUndoAnnotations = result.value.canUndo,
                canRedoAnnotations = result.value.canRedo,
                selectedAnnotationId = state.value.selectedAnnotationId?.takeIf { id -> result.value.annotations.any { it.id == id } },
            )
            is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
        }
    }

    private fun selectedAnnotation() = state.value.selectedAnnotationId?.let { id -> state.value.annotations.lastOrNull { it.id == id } }

    private fun applyEdit(edit: AnnotationEdit) = applyEdits(listOf(edit))

    private fun applyEdits(edits: List<AnnotationEdit>) {
        val openDocument = session.document ?: return
        if (!state.value.annotationEditingAllowed || edits.isEmpty()) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                var applied = false
                for (edit in edits) {
                    when (val result = withContext(Dispatchers.Default) { openDocument.applyAnnotationEdit(edit) }) {
                        is PdfCoreResult.Success -> applied = true
                        is PdfCoreResult.Failure -> {
                            if (applied) {
                                state.value = state.value.copy(isDirty = true, revision = state.value.revision + 1)
                                refresh(openDocument)
                            }
                            state.value = state.value.copy(status = userMessage(result.error))
                            return@withLock
                        }
                    }
                }
                state.value = state.value.copy(isDirty = true, revision = state.value.revision + 1)
                refresh(openDocument)
            }
        }
    }

    private fun applyHistory(undo: Boolean) {
        val openDocument = session.document ?: return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val result = withContext(Dispatchers.Default) { if (undo) openDocument.undoAnnotations() else openDocument.redoAnnotations() }
                when (result) {
                    is PdfCoreResult.Success -> if (result.value) {
                        state.value = state.value.copy(isDirty = true, revision = state.value.revision + 1)
                        refresh(openDocument)
                    }
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }

    private fun insertImageStamp(pageIndex: Int, imageBytes: ByteArray, anchor: AnnotationPoint) {
        val openDocument = session.document ?: return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                when (val placement = withContext(Dispatchers.Default) { openDocument.stampPlacement(imageBytes, anchor) }) {
                    is PdfCoreResult.Success -> when (val result = withContext(Dispatchers.Default) { openDocument.insertImageStamp(pageIndex, imageBytes, placement.value) }) {
                        is PdfCoreResult.Success -> {
                            state.value = state.value.copy(isDirty = true, revision = state.value.revision + 1)
                            refresh(openDocument)
                        }
                        is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                    }
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(placement.error))
                }
            }
        }
    }
}
