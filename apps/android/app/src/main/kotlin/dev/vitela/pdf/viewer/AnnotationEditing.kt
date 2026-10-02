package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationColor
import dev.vitela.pdf.core.AnnotationEdit
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * The annotation toolbar and its gestures, plus undo/redo of the shared edit
 * log. A text selection is an input here: the markup tools turn it into
 * annotations, and a tap elsewhere clears it.
 *
 * [afterHistory] runs after every undo or redo that moved the log, with the
 * document lane held, for the features that show something of the log other
 * than annotations — the Form fields panel re-reads its values there.
 */
internal class AnnotationEditing(
    private val session: ViewerSession,
    private val selection: TextSelecting,
    private val layout: PageLayout,
    private val afterHistory: suspend (PdfDocument) -> Unit = {},
) {
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

    /**
     * Previous/Next annotation: selects the neighbour in snapshot order and asks
     * the reader to reveal it. Only a selection and a scroll, never an edit, so
     * it works on a document that forbids annotating.
     */
    fun step(forward: Boolean) {
        val current = state.value
        if (!annotationNavigationEnabled(current)) return
        val target = annotationStep(current.annotations, current.selectedAnnotationId, forward) ?: return
        val position = current.annotations.indexOf(target) + 1
        state.value = current.copy(
            selectedAnnotationId = target.id,
            annotationReveal = annotationReveal(target),
            status = "Annotation $position of ${current.annotations.size}.",
        )
    }

    /** Consumed by the reader once it has scrolled, so the reveal fires once. */
    fun consumeReveal() {
        if (state.value.annotationReveal != null) state.value = state.value.copy(annotationReveal = null)
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
        } else if (tool == AnnotationTool.TextNote) {
            // Nothing reaches the core yet: the prompt asks for the text first.
            val rect = requireNotNull(placementAnnotation(tool, pageIndex, origin, current).rect)
            state.value = state.value.copy(notePlacement = NotePlacement(pageIndex, rect))
        } else {
            applyEdit(AnnotationEdit.Add(placementAnnotation(tool, pageIndex, origin, current, points)))
        }
        state.value = state.value.copy(activeAnnotationTool = AnnotationTool.Pointer)
    }

    /**
     * The Note prompt's **Add**: records the note with [text] exactly as typed,
     * as one undoable edit. [documentId] is the document the prompt was built
     * for — a prompt left over from a replaced document adds nothing — and a
     * blank text keeps the prompt open, since a note must say something.
     */
    fun addNote(documentId: Long, text: String) {
        val current = state.value
        val placement = current.notePlacement ?: return
        if (current.documentId != documentId || text.isBlank()) return
        state.value = current.copy(notePlacement = null)
        applyEdit(AnnotationEdit.Add(Annotation(0, placement.pageIndex, AnnotationKind.TextNote, placement.rect, null, contents = text)))
    }

    /**
     * **Read note**: shows the selected note's text exactly as the core holds
     * it. Not an edit — no document lane, no history, no dirty flag — so it
     * works when annotating is forbidden.
     */
    fun readNote() {
        val note = readableNote(state.value) ?: return
        state.value = state.value.copy(noteReading = NoteReading(note.pageIndex, note.contents.orEmpty()))
    }

    fun closeNoteReading() {
        if (state.value.noteReading != null) state.value = state.value.copy(noteReading = null)
    }

    /** The Note prompt's **Cancel**: no annotation, no undo step. */
    fun cancelNote() {
        if (state.value.notePlacement == null) return
        state.value = state.value.copy(notePlacement = null, status = NOTE_PLACEMENT_CANCELED)
    }

    fun selectImageStamp(bytes: ByteArray, prompt: String = "Tap a page to place the image stamp.") {
        if (!state.value.annotationEditingAllowed) return
        stampBytes = bytes
        state.value = state.value.copy(activeAnnotationTool = AnnotationTool.Stamp, status = prompt)
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

    /**
     * **Resize**: opens the dimensions dialog on the selected annotation. Only
     * one with a rectangle — ink has none — and never over the grid.
     */
    fun openResizer() {
        val current = state.value
        if (!current.annotationEditingAllowed || current.organize != null) return
        val selected = selectedAnnotation() ?: return
        val rect = selected.rect ?: return
        state.value = current.copy(annotationResizer = AnnotationResizer(selected.id, selected.pageIndex, rect))
    }

    /** The dialog's **Cancel**: no edit, so redo survives. */
    fun cancelResizer() {
        if (state.value.annotationResizer != null) state.value = state.value.copy(annotationResizer = null)
    }

    /**
     * The dialog's **Resize**: [width] by [height] points as typed, keeping the
     * origin, for the document [documentId] the dialog was built for. A size
     * that is not finite and positive keeps the dialog open with what was
     * typed; an unchanged one records nothing, so redo survives. The dialog is
     * spent before the core answers, so a second tap finds nothing open, and
     * an annotation no longer as the dialog found it is left alone.
     */
    fun resize(documentId: Long, width: String, height: String) {
        val current = state.value
        val resizer = current.annotationResizer ?: return
        if (current.documentId != documentId) return
        val to = sizedAnnotationRect(resizer.rect, typedPoints(width), typedPoints(height))
        if (to == null) {
            state.value = current.copy(annotationResizer = resizer.copy(width = width, height = height, error = ANNOTATION_SIZE_INVALID))
            return
        }
        val selected = selectedAnnotation()
        val status = when {
            !current.annotationEditingAllowed || selected?.id != resizer.id || selected.rect != resizer.rect -> ANNOTATION_CHANGED
            to == resizer.rect -> ANNOTATION_SIZE_UNCHANGED
            else -> null
        }
        state.value = current.copy(annotationResizer = null, status = status ?: current.status)
        if (status == null) applyEdits(listOf(AnnotationEdit.Resize(resizer.id, to)), done = ANNOTATION_RESIZED)
    }

    /**
     * **Move to**: opens the position dialog on the selected annotation, at
     * the bottom-left of its bounds — ink included, through its points. Never
     * over the grid.
     */
    fun openPositioner() {
        val current = state.value
        if (!current.annotationEditingAllowed || current.organize != null) return
        val selected = selectedAnnotation() ?: return
        val bounds = selected.bounds ?: return
        state.value = current.copy(annotationPositioner = AnnotationPositioner(selected, bounds))
    }

    /** The dialog's **Cancel**: no edit, so redo survives. */
    fun cancelPositioner() {
        if (state.value.annotationPositioner != null) state.value = state.value.copy(annotationPositioner = null)
    }

    /**
     * The dialog's **Move**: puts the bottom-left at [x], [y] points as typed,
     * size kept, for the document [documentId] the dialog was built for. A
     * coordinate that is not finite keeps the dialog open with what was typed;
     * an unchanged position records nothing, so redo survives. The dialog is
     * spent before the core answers, and an annotation no longer as the dialog
     * found it is left alone.
     */
    fun position(documentId: Long, x: String, y: String) {
        val current = state.value
        val positioner = current.annotationPositioner ?: return
        if (current.documentId != documentId) return
        val offset = positionOffset(positioner.bounds, typedPoints(x), typedPoints(y))
        if (offset == null) {
            state.value = current.copy(annotationPositioner = positioner.copy(x = x, y = y, error = ANNOTATION_POSITION_INVALID))
            return
        }
        val status = when {
            !current.annotationEditingAllowed || selectedAnnotation() != positioner.annotation -> ANNOTATION_POSITION_CHANGED
            offset.x == 0.0 && offset.y == 0.0 -> ANNOTATION_POSITION_UNCHANGED
            else -> null
        }
        state.value = current.copy(annotationPositioner = null, status = status ?: current.status)
        if (status == null) applyEdits(listOf(AnnotationEdit.Move(positioner.annotation.id, offset.x, offset.y)), done = ANNOTATION_MOVED)
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
        when (val result = withContext(session.compute) { openDocument.annotations() }) {
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

    /** [done], when given, is the status line once every edit landed; a refusal reports itself instead. */
    private fun applyEdits(edits: List<AnnotationEdit>, done: String? = null) {
        val openDocument = session.document ?: return
        if (!state.value.annotationEditingAllowed || edits.isEmpty()) return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                var applied = false
                for (edit in edits) {
                    when (val result = withContext(session.compute) { openDocument.applyAnnotationEdit(edit) }) {
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
                state.value = state.value.copy(isDirty = true, revision = state.value.revision + 1, status = done ?: state.value.status)
                refresh(openDocument)
            }
        }
    }

    private fun applyHistory(undo: Boolean) {
        val openDocument = session.document ?: return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument) return@withLock
                val result = withContext(session.compute) { if (undo) openDocument.undoAnnotations() else openDocument.redoAnnotations() }
                when (result) {
                    is PdfCoreResult.Success -> if (result.value) {
                        state.value = state.value.copy(isDirty = true, revision = state.value.revision + 1)
                        // An undo may have restored a page a move or delete took away,
                        // or a value a fill replaced: either way the preview is stale.
                        if (layout.edited) layout.reread(openDocument) else if (layout.redrawn) layout.redraw(openDocument)
                        refresh(openDocument)
                        afterHistory(openDocument)
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
                when (val placement = withContext(session.compute) { openDocument.stampPlacement(imageBytes, anchor) }) {
                    is PdfCoreResult.Success -> when (val result = withContext(session.compute) { openDocument.insertImageStamp(pageIndex, imageBytes, placement.value) }) {
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
