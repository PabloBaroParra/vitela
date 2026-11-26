package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.ContentTextRun
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.TextRunLook
import dev.vitela.pdf.core.TextRunLookState
import dev.vitela.pdf.core.TextRunRestyle

/**
 * [RetypableDocument]'s pages, whose runs also have a look. A run named in
 * [looks] starts with it — null for one whose look cannot be read — and every
 * other run starts plain. A restyle sets each run's look to its `to`, keeps its
 * id like the core, and is one undo step of its own; an undo takes back the last
 * restyle before any of the base document's edits, which is all these tests mix.
 */
internal class StylableDocument(
    looks: Map<Long, TextRunLookState?> = emptyMap(),
    private val base: RetypableDocument = RetypableDocument(),
) : PdfDocument by base {
    /** Refuses every restyle with this, as the core refuses one. */
    var restyleRefusal: PdfCoreError? = null

    /** Each restyle call the core accepted, as the shell sent it. */
    val restyles = mutableListOf<List<TextRunRestyle>>()

    /** Each run whose look was asked, in order. */
    val lookReads = mutableListOf<Long>()

    private var current: Map<Long, TextRunLookState?> = looks
    private val undoable = ArrayDeque<Map<Long, TextRunLookState?>>()
    private val redoable = ArrayDeque<Map<Long, TextRunLookState?>>()

    fun lookOf(id: Long): TextRunLookState? = if (id in current) current[id] else TextRunLookState(TextRunLook())

    override fun textRunLook(run: ContentTextRun): PdfCoreResult<TextRunLookState?> {
        lookReads += run.id
        return PdfCoreResult.Success(lookOf(run.id))
    }

    override fun restyleTextRuns(restyles: List<TextRunRestyle>): PdfCoreResult<Unit> {
        restyleRefusal?.let { return PdfCoreResult.Failure(it) }
        this.restyles += restyles
        undoable.addLast(current)
        redoable.clear()
        current = current + restyles.associate { it.run.id to (lookOf(it.run.id) ?: TextRunLookState(TextRunLook())).copy(look = it.to) }
        return PdfCoreResult.Success(Unit)
    }

    override fun annotations(): PdfCoreResult<AnnotationSnapshot> {
        val snapshot = base.annotations().value
        return PdfCoreResult.Success(snapshot.copy(canUndo = snapshot.canUndo || undoable.isNotEmpty(), canRedo = snapshot.canRedo || redoable.isNotEmpty()))
    }

    override fun undoAnnotations(): PdfCoreResult<Boolean> {
        val before = undoable.removeLastOrNull() ?: return base.undoAnnotations()
        redoable.addLast(current)
        current = before
        return PdfCoreResult.Success(true)
    }

    override fun redoAnnotations(): PdfCoreResult<Boolean> {
        val after = redoable.removeLastOrNull() ?: return base.redoAnnotations()
        undoable.addLast(current)
        current = after
        return PdfCoreResult.Success(true)
    }
}
