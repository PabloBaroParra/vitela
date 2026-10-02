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
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * **Resize** sets the selected annotation's width and height in points
 * (Windows #301): the PDF-space origin stays put, the core's resize command
 * records one undo step, and Cancel or unchanged dimensions leave history —
 * redo included — alone.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelAnnotationResizeTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    private val shape = Annotation(7, 0, AnnotationKind.Shape, AnnotationRect(40.0, 80.0, 100.0, 50.0), DEFAULT_ANNOTATION_COLOR)
    private val ink = Annotation(9, 0, AnnotationKind.Ink, null, DEFAULT_ANNOTATION_COLOR, listOf(AnnotationPoint(300.0, 300.0), AnnotationPoint(320.0, 330.0)))

    @Test
    fun openingTheDialogSnapshotsTheSelectedRectInPoints() = runTest {
        val (viewModel, _) = selected()

        viewModel.openAnnotationResizer()

        assertEquals(AnnotationResizer(7, 0, AnnotationRect(40.0, 80.0, 100.0, 50.0), "100", "50"), viewModel.state.value.annotationResizer)
    }

    @Test
    fun resizeKeepsTheOriginAndRecordsOneUndoableEdit() = runTest {
        val (viewModel, document) = selected()
        viewModel.openAnnotationResizer()

        viewModel.resizeAnnotation(viewModel.state.value.documentId, "123.5", "67,25")
        advanceUntilIdle()

        assertEquals(listOf(AnnotationEdit.Resize(7, AnnotationRect(40.0, 80.0, 123.5, 67.25))), document.edits)
        assertEquals(AnnotationRect(40.0, 80.0, 123.5, 67.25), viewModel.state.value.annotations.single { it.id == 7L }.rect)
        assertNull(viewModel.state.value.annotationResizer)
        assertEquals(ANNOTATION_RESIZED, viewModel.state.value.status)
        assertTrue(viewModel.state.value.canUndoAnnotations)
        assertTrue(viewModel.state.value.isDirty)
    }

    @Test
    fun invalidDimensionsKeepTheDialogWithWhatWasTyped() = runTest {
        val (viewModel, document) = selected()
        viewModel.openAnnotationResizer()

        for ((width, height) in listOf("0" to "10", "10" to "-1", "abc" to "10", "NaN" to "10", "Infinity" to "10")) {
            viewModel.resizeAnnotation(viewModel.state.value.documentId, width, height)
            advanceUntilIdle()
            val resizer = viewModel.state.value.annotationResizer
            assertEquals(width, resizer?.width)
            assertEquals(height, resizer?.height)
            assertEquals(ANNOTATION_SIZE_INVALID, resizer?.error)
        }
        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun unchangedDimensionsAndCancelPreserveRedo() = runTest {
        val (viewModel, document) = selected()
        viewModel.growSelected()
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()
        assertTrue(viewModel.state.value.canRedoAnnotations)

        viewModel.openAnnotationResizer()
        viewModel.resizeAnnotation(viewModel.state.value.documentId, "100", "50.00")
        advanceUntilIdle()
        assertNull(viewModel.state.value.annotationResizer)
        assertEquals(ANNOTATION_SIZE_UNCHANGED, viewModel.state.value.status)

        viewModel.openAnnotationResizer()
        viewModel.cancelAnnotationResizer()
        advanceUntilIdle()
        assertNull(viewModel.state.value.annotationResizer)

        assertEquals(1, document.edits.size)
        assertTrue(viewModel.state.value.canRedoAnnotations)
    }

    @Test
    fun aDialogBuiltForAnotherDocumentResizesNothing() = runTest {
        val (viewModel, document) = selected()
        viewModel.openAnnotationResizer()

        viewModel.resizeAnnotation(viewModel.state.value.documentId - 1, "200", "200")
        advanceUntilIdle()

        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun anAnnotationChangedWhileTheDialogWasOpenIsNotOverwritten() = runTest {
        val (viewModel, document) = selected()
        viewModel.growSelected()
        advanceUntilIdle()
        viewModel.openAnnotationResizer()
        // An undo lands under the open dialog and puts the old rectangle back.
        viewModel.undoAnnotations()
        advanceUntilIdle()

        viewModel.resizeAnnotation(viewModel.state.value.documentId, "200", "200")
        advanceUntilIdle()

        assertEquals(1, document.edits.size)
        assertEquals(shape.rect, viewModel.state.value.annotations.single().rect)
        assertNull(viewModel.state.value.annotationResizer)
        assertEquals(ANNOTATION_CHANGED, viewModel.state.value.status)
    }

    @Test
    fun aSecondTapOnResizeFindsTheDialogSpent() = runTest {
        val (viewModel, document) = selected()
        viewModel.openAnnotationResizer()

        viewModel.resizeAnnotation(viewModel.state.value.documentId, "200", "60")
        viewModel.resizeAnnotation(viewModel.state.value.documentId, "200", "60")
        advanceUntilIdle()

        assertEquals(1, document.edits.size)
    }

    @Test
    fun inkHasNoRectangleToResize() = runTest {
        val (viewModel, _) = opened(ResizableDocument(mutableListOf(shape, ink)))
        viewModel.selectAnnotation(0, AnnotationPoint(310.0, 310.0))
        assertEquals(9L, viewModel.state.value.selectedAnnotationId)

        viewModel.openAnnotationResizer()

        assertNull(viewModel.state.value.annotationResizer)
    }

    @Test
    fun aDocumentThatForbidsAnnotatingOpensNoDialog() = runTest {
        val (viewModel, _) = opened(ResizableDocument(mutableListOf(shape), editingAllowed = false))
        viewModel.selectAnnotation(0, AnnotationPoint(50.0, 90.0))

        viewModel.openAnnotationResizer()

        assertNull(viewModel.state.value.annotationResizer)
    }

    @Test
    fun theGridOpensNoDialog() = runTest {
        val (viewModel, _) = selected()
        viewModel.openOrganize()

        viewModel.openAnnotationResizer()

        assertNull(viewModel.state.value.annotationResizer)
    }

    private suspend fun TestScope.selected(): Pair<ViewerViewModel, ResizableDocument> {
        val (viewModel, document) = opened(ResizableDocument(mutableListOf(shape)))
        viewModel.selectAnnotation(0, AnnotationPoint(50.0, 90.0))
        assertEquals(7L, viewModel.state.value.selectedAnnotationId)
        return viewModel to document
    }

    private suspend fun TestScope.opened(document: ResizableDocument): Pair<ViewerViewModel, ResizableDocument> {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel to document
    }
}

/** Applies resizes to its own list, with an undo/redo log of whole snapshots like the core's. */
private class ResizableDocument(
    val annotationList: MutableList<Annotation>,
    private val editingAllowed: Boolean = true,
    private val base: PdfDocument = OrganizableDocument(pageCount = 2),
) : PdfDocument by base {
    val edits = mutableListOf<AnnotationEdit>()
    private val undoable = ArrayDeque<List<Annotation>>()
    private val redoable = ArrayDeque<List<Annotation>>()

    override fun annotations() = PdfCoreResult.Success(AnnotationSnapshot(annotationList.toList(), editingAllowed, undoable.isNotEmpty(), redoable.isNotEmpty()))

    override fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> {
        val resize = edit as AnnotationEdit.Resize
        undoable.addLast(annotationList.toList())
        redoable.clear()
        edits += edit
        annotationList.replaceAll { if (it.id == resize.id) it.copy(rect = resize.rect) else it }
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
