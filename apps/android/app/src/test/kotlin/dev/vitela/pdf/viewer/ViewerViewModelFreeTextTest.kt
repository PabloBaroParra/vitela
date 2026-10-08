package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationEdit
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.CoreFailure
import dev.vitela.pdf.core.FreeTextLayout
import dev.vitela.pdf.core.FreeTextLine
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.toPdfCoreError
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
 * FreeText on Android: the armed tool asks for the text before anything
 * reaches the core, Add stays disabled for blank text, Cancel leaves no undo
 * step, a stale document is ignored, and a character Helvetica cannot show
 * keeps the dialog open with what was typed. Editing the text of a selected
 * box is its own undo step.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelFreeTextTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    private val box = Annotation(
        7, 0, AnnotationKind.FreeText, AnnotationRect(40.0, 80.0, 100.0, 50.0), null,
        contents = "uno", layout = FreeTextLayout(12.0, listOf(FreeTextLine("uno", 2.0, 12.6))),
    )
    private val note = Annotation(9, 0, AnnotationKind.TextNote, AnnotationRect(200.0, 200.0, 10.0, 10.0), null, contents = "n")

    @Test
    fun anArmedTapAsksForTextBeforeAnyEditAndDisarmsTheTool() = runTest {
        val (viewModel, document) = opened()
        place(viewModel)
        advanceUntilIdle()

        assertEquals(
            FreeTextDraft(0, FreeTextTarget.Place(AnnotationRect(10.0, 100.0, 200.0, 50.0))),
            viewModel.state.value.freeText,
        )
        assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun theBoxIsClampedToThePageWhereTheTapLands() = runTest {
        val (viewModel, _) = opened()
        viewModel.setAnnotationTool(AnnotationTool.FreeText)
        viewModel.placeAnnotation(1, AnnotationPoint(290.0, 20.0), AnnotationPoint(290.0, 20.0))
        advanceUntilIdle()

        assertEquals(FreeTextTarget.Place(AnnotationRect(100.0, 0.0, 200.0, 50.0)), viewModel.state.value.freeText?.target)
        assertEquals(1, viewModel.state.value.freeText?.pageIndex)
    }

    @Test
    fun addRecordsTheTypedTextVerbatimAsOneUndoableEdit() = runTest {
        val (viewModel, document) = opened()
        place(viewModel)
        viewModel.confirmFreeText(viewModel.state.value.documentId, "  Año ¿qué?\nsegunda  ")
        advanceUntilIdle()

        val added = (document.edits.single() as AnnotationEdit.Add).annotation
        assertEquals(AnnotationKind.FreeText, added.kind)
        assertEquals(AnnotationRect(10.0, 100.0, 200.0, 50.0), added.rect)
        assertEquals("  Año ¿qué?\nsegunda  ", added.contents)
        assertNull(viewModel.state.value.freeText)
        assertTrue(viewModel.state.value.canUndoAnnotations)
        assertTrue(viewModel.state.value.isDirty)
        assertEquals(FREE_TEXT_ADDED, viewModel.state.value.status)
        // The snapshot the core hands back is what the overlay draws.
        assertEquals("  Año ¿qué?\nsegunda  ", viewModel.state.value.annotations.single().contents)
    }

    @Test
    fun cancelLeavesNoAnnotationAndNoUndoStep() = runTest {
        val (viewModel, document) = opened()
        place(viewModel)
        viewModel.cancelFreeText()
        advanceUntilIdle()

        assertNull(viewModel.state.value.freeText)
        assertEquals(FREE_TEXT_CANCELED, viewModel.state.value.status)
        assertTrue(document.edits.isEmpty())
        assertFalse(viewModel.state.value.canUndoAnnotations)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun blankTextAddsNothingAndKeepsTheDialog() = runTest {
        val (viewModel, document) = opened()
        place(viewModel)
        for (blank in listOf("", "   ", " \n\t")) viewModel.confirmFreeText(viewModel.state.value.documentId, blank)
        advanceUntilIdle()

        assertTrue(document.edits.isEmpty())
        assertEquals(0, viewModel.state.value.freeText?.pageIndex)
    }

    @Test
    fun aDialogBuiltForAnotherDocumentAddsNothing() = runTest {
        val (viewModel, document) = opened()
        place(viewModel)
        viewModel.confirmFreeText(viewModel.state.value.documentId - 1, "Stale")
        advanceUntilIdle()

        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun aSecondTapOnAddWhileTheCoreIsWorkingSendsOneEdit() = runTest {
        val (viewModel, document) = opened()
        place(viewModel)
        val id = viewModel.state.value.documentId
        viewModel.confirmFreeText(id, "once")
        viewModel.confirmFreeText(id, "once")
        advanceUntilIdle()

        assertEquals(1, document.edits.size)
    }

    @Test
    fun aCharacterHelveticaCannotShowKeepsTheDialogOpenNamingIt() = runTest {
        val (viewModel, document) = opened()
        place(viewModel)
        viewModel.confirmFreeText(viewModel.state.value.documentId, "Hola 日本")
        advanceUntilIdle()

        val draft = requireNotNull(viewModel.state.value.freeText)
        assertEquals("This text's font cannot show \"日\". Try different characters.", draft.error)
        assertFalse(draft.busy)
        assertTrue(document.edits.isEmpty())
        assertFalse(viewModel.state.value.isDirty)

        // The reader fixes the text and the same dialog completes.
        viewModel.confirmFreeText(viewModel.state.value.documentId, "Hola")
        advanceUntilIdle()
        assertNull(viewModel.state.value.freeText)
        assertEquals("Hola", (document.edits.single() as AnnotationEdit.Add).annotation.contents)
    }

    @Test
    fun theToolIsRefusedOnADocumentThatForbidsAnnotating() = runTest {
        val (viewModel, document) = opened(FreeTextDocument(editingAllowed = false))
        viewModel.setAnnotationTool(AnnotationTool.FreeText)
        viewModel.placeAnnotation(0, AnnotationPoint(10.0, 150.0), AnnotationPoint(10.0, 150.0))
        advanceUntilIdle()

        assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
        assertNull(viewModel.state.value.freeText)
        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun editTextOpensTheDialogPrefilledWithTheBoxsText() = runTest {
        val (viewModel, _) = selected()
        viewModel.openFreeTextEditor()

        assertEquals(FreeTextDraft(0, FreeTextTarget.Retype(7, "uno")), viewModel.state.value.freeText)
    }

    @Test
    fun confirmingNewTextIsOneSetContentsEditThatUndoRestores() = runTest {
        val (viewModel, document) = selected()
        viewModel.openFreeTextEditor()
        viewModel.confirmFreeText(viewModel.state.value.documentId, "dos")
        advanceUntilIdle()

        assertEquals(listOf(AnnotationEdit.SetContents(7, "dos")), document.edits)
        assertEquals("dos", viewModel.state.value.annotations.single { it.id == 7L }.contents)
        assertNull(viewModel.state.value.freeText)
        assertEquals(FREE_TEXT_EDITED, viewModel.state.value.status)

        viewModel.undoAnnotations()
        advanceUntilIdle()
        assertEquals("uno", viewModel.state.value.annotations.single { it.id == 7L }.contents)
    }

    @Test
    fun unchangedTextRecordsNothingSoRedoSurvives() = runTest {
        val (viewModel, document) = selected()
        viewModel.openFreeTextEditor()
        viewModel.confirmFreeText(viewModel.state.value.documentId, "uno")
        advanceUntilIdle()

        assertTrue(document.edits.isEmpty())
        assertNull(viewModel.state.value.freeText)
        assertEquals(FREE_TEXT_UNCHANGED, viewModel.state.value.status)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun cancellingTheEditorChangesNothing() = runTest {
        val (viewModel, document) = selected()
        viewModel.openFreeTextEditor()
        viewModel.cancelFreeText()
        advanceUntilIdle()

        assertNull(viewModel.state.value.freeText)
        assertTrue(document.edits.isEmpty())
        assertEquals("uno", viewModel.state.value.annotations.single { it.id == 7L }.contents)
    }

    @Test
    fun anEditorBuiltForAnotherDocumentIsDiscarded() = runTest {
        val (viewModel, document) = selected()
        viewModel.openFreeTextEditor()
        viewModel.confirmFreeText(viewModel.state.value.documentId - 1, "dos")
        advanceUntilIdle()

        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun aBoxDeletedUnderTheOpenEditorIsLeftAlone() = runTest {
        val (viewModel, document) = selected()
        viewModel.openFreeTextEditor()
        viewModel.deleteSelected()
        advanceUntilIdle()
        viewModel.confirmFreeText(viewModel.state.value.documentId, "dos")
        advanceUntilIdle()

        assertEquals(listOf<AnnotationEdit>(AnnotationEdit.Remove(7)), document.edits)
        assertNull(viewModel.state.value.freeText)
        assertEquals(ANNOTATION_CHANGED_UNDER_EDITOR, viewModel.state.value.status)
    }

    @Test
    fun anEditWithACharacterHelveticaCannotShowKeepsTheEditorOpen() = runTest {
        val (viewModel, document) = selected()
        viewModel.openFreeTextEditor()
        viewModel.confirmFreeText(viewModel.state.value.documentId, "uno 日")
        advanceUntilIdle()

        assertEquals("This text's font cannot show \"日\". Try different characters.", viewModel.state.value.freeText?.error)
        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun editTextIsRefusedForAnotherKindOfAnnotation() = runTest {
        val (viewModel, _) = opened(FreeTextDocument(mutableListOf(box, note)))
        viewModel.selectAnnotation(0, AnnotationPoint(205.0, 205.0))
        assertEquals(9L, viewModel.state.value.selectedAnnotationId)

        viewModel.openFreeTextEditor()

        assertNull(viewModel.state.value.freeText)
    }

    @Test
    fun editTextIsRefusedWhenAnnotatingIsForbiddenButTheBoxStaysSelectable() = runTest {
        val (viewModel, document) = opened(FreeTextDocument(mutableListOf(box), editingAllowed = false))
        viewModel.selectAnnotation(0, AnnotationPoint(50.0, 90.0))
        viewModel.openFreeTextEditor()
        advanceUntilIdle()

        assertEquals(7L, viewModel.state.value.selectedAnnotationId)
        assertNull(viewModel.state.value.freeText)
        assertTrue(document.edits.isEmpty())
    }

    @Test
    fun aDragResizeBelowTheMinimumSendsAClampedRect() = runTest {
        val (viewModel, document) = selected()
        viewModel.resizeSelected(HandleCorner.TopRight, AnnotationPoint(41.0, 81.0))
        advanceUntilIdle()

        assertEquals(listOf<AnnotationEdit>(AnnotationEdit.Resize(7, AnnotationRect(40.0, 80.0, 16.0, 17.8))), document.edits)
    }

    private fun place(viewModel: ViewerViewModel) {
        viewModel.setAnnotationTool(AnnotationTool.FreeText)
        viewModel.placeAnnotation(0, AnnotationPoint(10.0, 150.0), AnnotationPoint(10.0, 150.0))
    }

    private suspend fun TestScope.selected(): Pair<ViewerViewModel, FreeTextDocument> {
        val opened = opened(FreeTextDocument(mutableListOf(box)))
        opened.first.selectAnnotation(0, AnnotationPoint(50.0, 90.0))
        assertEquals(7L, opened.first.state.value.selectedAnnotationId)
        return opened
    }

    private suspend fun TestScope.opened(document: FreeTextDocument = FreeTextDocument()): Pair<ViewerViewModel, FreeTextDocument> {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel to document
    }
}

/** Applies Add, SetContents, Resize and Remove to its own list, refusing what Helvetica cannot show like the core. */
private class FreeTextDocument(
    val annotationList: MutableList<Annotation> = mutableListOf(),
    private val editingAllowed: Boolean = true,
    private val base: PdfDocument = OrganizableDocument(pageCount = 2),
) : PdfDocument by base {
    val edits = mutableListOf<AnnotationEdit>()
    private val undoable = ArrayDeque<List<Annotation>>()
    private val redoable = ArrayDeque<List<Annotation>>()
    private var nextId = 100L

    override val pageSizes: List<PageSize> = List(2) { PageSize(300.0, 400.0) }

    override fun annotations() = PdfCoreResult.Success(AnnotationSnapshot(annotationList.toList(), editingAllowed, undoable.isNotEmpty(), redoable.isNotEmpty()))

    override fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> {
        val text = when (edit) {
            is AnnotationEdit.Add -> edit.annotation.contents
            is AnnotationEdit.SetContents -> edit.contents
            else -> null
        }
        text?.firstOrNull { it.code > 0xFF }?.let { return PdfCoreResult.Failure(CoreFailure.EncodingGap.toPdfCoreError(it.toString())) }
        undoable.addLast(annotationList.toList())
        redoable.clear()
        edits += edit
        when (edit) {
            is AnnotationEdit.Add -> annotationList += edit.annotation.copy(id = nextId++, layout = layoutOf(edit.annotation.contents.orEmpty()))
            is AnnotationEdit.SetContents -> annotationList.replaceAll { if (it.id == edit.id) it.copy(contents = edit.contents, layout = layoutOf(edit.contents)) else it }
            is AnnotationEdit.Resize -> annotationList.replaceAll { if (it.id == edit.id) it.copy(rect = edit.rect) else it }
            is AnnotationEdit.Remove -> annotationList.removeAll { it.id == edit.id }
            else -> error("unexpected $edit")
        }
        return PdfCoreResult.Success(Unit)
    }

    private fun layoutOf(text: String) = FreeTextLayout(12.0, text.lines().mapIndexed { i, line -> FreeTextLine(line, 2.0, 12.6 + 13.8 * i) })

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
