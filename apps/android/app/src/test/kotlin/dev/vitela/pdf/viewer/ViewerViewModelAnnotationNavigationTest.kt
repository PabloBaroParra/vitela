package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationRect
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

/** Previous/Next annotation: selects in snapshot order and reveals it, and is never a document edit. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelAnnotationNavigationTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    private val notes = listOf(
        Annotation(5, 1, AnnotationKind.Highlight, AnnotationRect(10.0, 20.0, 30.0, 8.0), DEFAULT_ANNOTATION_COLOR),
        Annotation(8, 3, AnnotationKind.Shape, AnnotationRect(40.0, 50.0, 60.0, 70.0), DEFAULT_ANNOTATION_COLOR),
    )

    @Test
    fun nextSelectsTheFirstAnnotationAndRevealsItsGeometry() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 5, annotationList = notes))

        viewModel.stepAnnotation(forward = true)

        val state = viewModel.state.value
        assertEquals(5L, state.selectedAnnotationId)
        assertEquals(AnnotationReveal(1, AnnotationRect(10.0, 20.0, 30.0, 8.0)), state.annotationReveal)
        assertEquals("Annotation 1 of 2.", state.status)
    }

    @Test
    fun previousEntersAtTheLastAndNextWrapsToTheFirst() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 5, annotationList = notes))

        viewModel.stepAnnotation(forward = false)
        assertEquals(8L, viewModel.state.value.selectedAnnotationId)
        assertEquals(3, viewModel.state.value.annotationReveal?.pageIndex)

        viewModel.stepAnnotation(forward = true)
        assertEquals(5L, viewModel.state.value.selectedAnnotationId)
    }

    @Test
    fun theRevealFiresOnceThenTheUserOwnsTheScroll() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 5, annotationList = notes))
        viewModel.stepAnnotation(forward = true)

        viewModel.consumeAnnotationReveal()

        assertNull(viewModel.state.value.annotationReveal)
        assertEquals(5L, viewModel.state.value.selectedAnnotationId)
    }

    @Test
    fun navigationWorksWhenEditingIsForbiddenAndLeavesHistoryAlone() = runTest {
        val document = OrganizableDocument(pageCount = 5, annotationList = notes, annotationEditingAllowed = false)
        val viewModel = openedWith(document)

        viewModel.stepAnnotation(forward = true)
        advanceUntilIdle()

        assertEquals(5L, viewModel.state.value.selectedAnnotationId)
        assertTrue(document.edits.isEmpty())
        assertFalse(viewModel.state.value.isDirty)
        assertFalse(viewModel.state.value.canUndoAnnotations)
    }

    @Test
    fun theGridIgnoresNavigation() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 5, annotationList = notes))
        viewModel.openOrganize()

        viewModel.stepAnnotation(forward = true)

        assertNull(viewModel.state.value.selectedAnnotationId)
        assertNull(viewModel.state.value.annotationReveal)
    }

    @Test
    fun aDocumentWithoutAnnotationsIsLeftAlone() = runTest {
        val viewModel = openedWith(OrganizableDocument(pageCount = 5))

        viewModel.stepAnnotation(forward = true)

        assertNull(viewModel.state.value.selectedAnnotationId)
        assertNull(viewModel.state.value.annotationReveal)
    }

    private suspend fun TestScope.openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
