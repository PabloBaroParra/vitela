package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationColor
import dev.vitela.pdf.core.AnnotationEdit
import dev.vitela.pdf.core.AnnotationKind
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
 * **Read note** shows the selected note's text exactly as the core holds it
 * (Windows #296). Reading is not an edit: it works on a document that forbids
 * annotating and leaves the edit log, the dirty flag and the selection alone.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelNoteReadingTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun readingShowsTheCoreTextVerbatimWithoutEditing() = runTest {
        val (viewModel, document) = opened()
        viewModel.stepAnnotation(true)
        assertTrue(readableNote(viewModel.state.value) != null)
        viewModel.readNote()
        advanceUntilIdle()

        assertEquals(NoteReading(0, "  First line\rSecond line — 日本語  "), viewModel.state.value.noteReading)
        assertFalse(viewModel.state.value.annotationEditingAllowed)
        assertFalse(viewModel.state.value.canUndoAnnotations)
        assertFalse(viewModel.state.value.isDirty)
        assertEquals(7L, viewModel.state.value.selectedAnnotationId)
        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun anEmptyNoteReadsAsEmptyText() = runTest {
        val (viewModel, _) = opened()
        viewModel.stepAnnotation(true)
        viewModel.stepAnnotation(true)
        viewModel.readNote()

        assertEquals(NoteReading(1, ""), viewModel.state.value.noteReading)
    }

    @Test
    fun aSelectionThatIsNotANoteReadsNothing() = runTest {
        val (viewModel, _) = opened()
        viewModel.stepAnnotation(false)
        assertNull(readableNote(viewModel.state.value))
        viewModel.readNote()

        assertNull(viewModel.state.value.noteReading)
    }

    @Test
    fun withoutASelectionNothingIsRead() = runTest {
        val (viewModel, _) = opened()
        assertNull(readableNote(viewModel.state.value))
        viewModel.readNote()

        assertNull(viewModel.state.value.noteReading)
    }

    @Test
    fun closingDropsTheReadingAndKeepsTheSelection() = runTest {
        val (viewModel, document) = opened()
        viewModel.stepAnnotation(true)
        viewModel.readNote()
        viewModel.closeNoteReading()
        advanceUntilIdle()

        assertNull(viewModel.state.value.noteReading)
        assertEquals(7L, viewModel.state.value.selectedAnnotationId)
        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun replacingTheDocumentClosesTheReading() = runTest {
        val (viewModel, _) = opened()
        viewModel.stepAnnotation(true)
        viewModel.readNote()
        viewModel.open("b.pdf", byteArrayOf(2))
        viewModel.state.first { it.title == "b.pdf" && !it.isLoading }
        advanceUntilIdle()

        assertNull(viewModel.state.value.noteReading)
    }

    private suspend fun TestScope.opened(): Pair<ViewerViewModel, NotesDocument> {
        val document = NotesDocument()
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document, NotesDocument()))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel to document
    }

    @Test
    fun savedCommentReadsAndRevealsWithoutMakingItEditable() = runTest {
        val (viewModel, document) = opened()
        val comment = viewModel.state.value.comments.single()
        viewModel.readComment(comment)

        assertEquals(NoteReading(1, "External — 日本語", "Author", "D:20261008"), viewModel.state.value.noteReading)
        assertEquals(AnnotationReveal(1, comment.rect), viewModel.state.value.annotationReveal)
        assertNull(viewModel.state.value.selectedAnnotationId)
        assertFalse(viewModel.state.value.isDirty)
        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun staleCommentDoesNotOpenInAnotherDocument() = runTest {
        val (viewModel, _) = opened()
        viewModel.readComment(viewModel.state.value.comments.single().copy(contents = "stale"))
        assertNull(viewModel.state.value.noteReading)
    }
}

/** A restricted document with two session notes (one blank) and a shape; edits are recorded, never expected. */
private class NotesDocument(private val base: PdfDocument = RetypableDocument()) : PdfDocument by base {
    val edits = mutableListOf<AnnotationEdit>()
    private val notes = listOf(
        Annotation(7, 0, AnnotationKind.TextNote, AnnotationRect(10.0, 20.0, 30.0, 40.0), null, contents = "  First line\rSecond line — 日本語  "),
        Annotation(8, 1, AnnotationKind.TextNote, AnnotationRect(50.0, 20.0, 30.0, 40.0), null, contents = ""),
        Annotation(9, 0, AnnotationKind.Shape, AnnotationRect(90.0, 20.0, 30.0, 40.0), AnnotationColor(255, 220, 0)),
    )

    override fun annotations() = PdfCoreResult.Success(AnnotationSnapshot(notes, false, false, false,
        listOf(dev.vitela.pdf.core.PdfComment(1, AnnotationRect(10.0, 20.0, 30.0, 40.0), "External — 日本語", "Author", "D:20261008", null))))

    override fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> {
        edits += edit
        return PdfCoreResult.Success(Unit)
    }
}
