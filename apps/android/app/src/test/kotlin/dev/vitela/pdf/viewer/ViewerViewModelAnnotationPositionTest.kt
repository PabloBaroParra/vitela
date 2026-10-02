package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationEdit
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * **Move to** sets the selected annotation's bottom-left X/Y in unrotated PDF
 * points (Windows #303): the size stays, ink moves every point by the same
 * offset, the core's move command records one undo step, and Cancel or an
 * unchanged position leave history — redo included — alone.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelAnnotationPositionTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    private val shape = Annotation(7, 0, AnnotationKind.Shape, AnnotationRect(23.5, 47.25, 100.0, 40.0), DEFAULT_ANNOTATION_COLOR)
    private val ink = Annotation(9, 0, AnnotationKind.Ink, null, DEFAULT_ANNOTATION_COLOR, listOf(AnnotationPoint(300.0, 300.0), AnnotationPoint(320.0, 330.0)))

    @Test
    fun openingTheDialogSnapshotsTheBottomLeftInPoints() = runTest {
        val (viewModel, _) = selected()

        viewModel.openAnnotationPositioner()

        assertEquals(AnnotationPositioner(shape, AnnotationRect(23.5, 47.25, 100.0, 40.0), "23.5", "47.25"), viewModel.state.value.annotationPositioner)
    }

    @Test
    fun moveKeepsTheSizeAndRecordsOneUndoableEdit() = runTest {
        val (viewModel, document) = selected()
        viewModel.openAnnotationPositioner()

        viewModel.positionAnnotation(viewModel.state.value.documentId, "-10", "0,5")
        advanceUntilIdle()

        assertEquals(listOf(AnnotationEdit.Move(7, -33.5, -46.75)), document.edits)
        assertEquals(AnnotationRect(-10.0, 0.5, 100.0, 40.0), viewModel.state.value.annotations.single { it.id == 7L }.rect)
        assertNull(viewModel.state.value.annotationPositioner)
        assertEquals(ANNOTATION_MOVED, viewModel.state.value.status)
        assertTrue(viewModel.state.value.canUndoAnnotations)
        assertTrue(viewModel.state.value.isDirty)
    }

    @Test
    fun inkMovesEveryPointByTheBoundsOffset() = runTest {
        val (viewModel, document) = opened(MovableDocument(mutableListOf(shape, ink)))
        viewModel.selectAnnotation(0, AnnotationPoint(310.0, 310.0))
        viewModel.openAnnotationPositioner()
        assertEquals("300", viewModel.state.value.annotationPositioner?.x)

        viewModel.positionAnnotation(viewModel.state.value.documentId, "-5", "0")
        advanceUntilIdle()

        assertEquals(listOf(AnnotationEdit.Move(9, -305.0, -300.0)), document.edits)
        assertEquals(listOf(AnnotationPoint(-5.0, 0.0), AnnotationPoint(15.0, 30.0)), viewModel.state.value.annotations.single { it.id == 9L }.points)
    }

    @Test
    fun invalidCoordinatesKeepTheDialogWithWhatWasTyped() = runTest {
        val (viewModel, document) = selected()
        viewModel.openAnnotationPositioner()

        for ((x, y) in listOf("abc" to "10", "10" to "", "NaN" to "10", "Infinity" to "10", "10" to "-Infinity")) {
            viewModel.positionAnnotation(viewModel.state.value.documentId, x, y)
            advanceUntilIdle()
            val positioner = viewModel.state.value.annotationPositioner
            assertEquals(x, positioner?.x)
            assertEquals(y, positioner?.y)
            assertEquals(ANNOTATION_POSITION_INVALID, positioner?.error)
        }
        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun anOffsetThatOverflowsIsRefused() = runTest {
        val far = shape.copy(rect = AnnotationRect(-1.7e308, 0.0, 10.0, 10.0))
        val (viewModel, document) = opened(MovableDocument(mutableListOf(far)))
        viewModel.selectAnnotation(0, AnnotationPoint(-1.7e308, 5.0))
        viewModel.openAnnotationPositioner()

        viewModel.positionAnnotation(viewModel.state.value.documentId, "1.7e308", "0")
        advanceUntilIdle()

        assertEquals(ANNOTATION_POSITION_INVALID, viewModel.state.value.annotationPositioner?.error)
        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun unchangedPositionAndCancelPreserveRedo() = runTest {
        val (viewModel, document) = selected()
        viewModel.growSelected()
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()
        assertTrue(viewModel.state.value.canRedoAnnotations)

        viewModel.openAnnotationPositioner()
        viewModel.positionAnnotation(viewModel.state.value.documentId, "23.50", "47.25")
        advanceUntilIdle()
        assertNull(viewModel.state.value.annotationPositioner)
        assertEquals(ANNOTATION_POSITION_UNCHANGED, viewModel.state.value.status)

        viewModel.openAnnotationPositioner()
        viewModel.cancelAnnotationPositioner()
        advanceUntilIdle()
        assertNull(viewModel.state.value.annotationPositioner)

        assertEquals(1, document.edits.size)
        assertTrue(viewModel.state.value.canRedoAnnotations)
    }

    @Test
    fun aDialogBuiltForAnotherDocumentMovesNothing() = runTest {
        val (viewModel, document) = selected()
        viewModel.openAnnotationPositioner()

        viewModel.positionAnnotation(viewModel.state.value.documentId - 1, "200", "200")
        advanceUntilIdle()

        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun anAnnotationChangedWhileTheDialogWasOpenIsNotMoved() = runTest {
        val (viewModel, document) = selected()
        viewModel.growSelected()
        advanceUntilIdle()
        viewModel.openAnnotationPositioner()
        // An undo lands under the open dialog and puts the old rectangle back.
        viewModel.undoAnnotations()
        advanceUntilIdle()

        viewModel.positionAnnotation(viewModel.state.value.documentId, "200", "200")
        advanceUntilIdle()

        assertEquals(1, document.edits.size)
        assertEquals(shape.rect, viewModel.state.value.annotations.single().rect)
        assertNull(viewModel.state.value.annotationPositioner)
        assertEquals(ANNOTATION_POSITION_CHANGED, viewModel.state.value.status)
    }

    @Test
    fun aSecondTapOnMoveFindsTheDialogSpent() = runTest {
        val (viewModel, document) = selected()
        viewModel.openAnnotationPositioner()

        viewModel.positionAnnotation(viewModel.state.value.documentId, "200", "60")
        viewModel.positionAnnotation(viewModel.state.value.documentId, "200", "60")
        advanceUntilIdle()

        assertEquals(1, document.edits.size)
    }

    @Test
    fun aDocumentThatForbidsAnnotatingOpensNoDialog() = runTest {
        val (viewModel, _) = opened(MovableDocument(mutableListOf(shape), editingAllowed = false))
        viewModel.selectAnnotation(0, AnnotationPoint(50.0, 60.0))

        viewModel.openAnnotationPositioner()

        assertNull(viewModel.state.value.annotationPositioner)
    }

    @Test
    fun theGridOpensNoDialog() = runTest {
        val (viewModel, _) = selected()
        viewModel.openOrganize()

        viewModel.openAnnotationPositioner()

        assertNull(viewModel.state.value.annotationPositioner)
    }

    private suspend fun TestScope.selected(): Pair<ViewerViewModel, MovableDocument> {
        val (viewModel, document) = opened(MovableDocument(mutableListOf(shape)))
        viewModel.selectAnnotation(0, AnnotationPoint(50.0, 60.0))
        assertEquals(7L, viewModel.state.value.selectedAnnotationId)
        return viewModel to document
    }

    private suspend fun TestScope.opened(document: MovableDocument): Pair<ViewerViewModel, MovableDocument> {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel to document
    }
}

/** Applies moves and resizes to its own list, with an undo/redo log of whole snapshots like the core's. */
private class MovableDocument(
    val annotationList: MutableList<Annotation>,
    private val editingAllowed: Boolean = true,
    private val base: PdfDocument = OrganizableDocument(pageCount = 2),
) : PdfDocument by base {
    val edits = mutableListOf<AnnotationEdit>()
    private val undoable = ArrayDeque<List<Annotation>>()
    private val redoable = ArrayDeque<List<Annotation>>()

    override fun annotations() = PdfCoreResult.Success(AnnotationSnapshot(annotationList.toList(), editingAllowed, undoable.isNotEmpty(), redoable.isNotEmpty()))

    override fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> {
        undoable.addLast(annotationList.toList())
        redoable.clear()
        edits += edit
        annotationList.replaceAll {
            when {
                edit is AnnotationEdit.Move && it.id == edit.id -> it.translated(edit.dx, edit.dy)
                edit is AnnotationEdit.Resize && it.id == edit.id -> it.copy(rect = edit.rect)
                else -> it
            }
        }
        return PdfCoreResult.Success(Unit)
    }

    override fun undoAnnotations(): PdfCoreResult<Boolean> = swap(undoable, redoable)
    override fun redoAnnotations(): PdfCoreResult<Boolean> = swap(redoable, undoable)

    private fun swap(from: ArrayDeque<List<Annotation>>, to: ArrayDeque<List<Annotation>>): PdfCoreResult<Boolean> {
        val restored = from.removeLastOrNull() ?: return PdfCoreResult.Success(false)
        to.addLast(annotationList.toList())
        annotationList.clear()
        annotationList.addAll(restored)
        return PdfCoreResult.Success(true)
    }
}
