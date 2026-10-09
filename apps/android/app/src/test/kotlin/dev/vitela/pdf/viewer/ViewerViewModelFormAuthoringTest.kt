package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.NewFormField
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCoreError
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

/** Placing new fields and moving existing ones from the Form fields panel, each one undoable edit of the shared log. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelFormAuthoringTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    private val page = PageSize(100.0, 200.0)

    @Test
    fun thePanelKnowsWhetherFieldsMayBeCreated() = runTest {
        val allowed = openedWithPanel(FillableDocument())
        assertTrue(allowed.state.value.formFields!!.authoringAllowed)

        val forbidden = openedWithPanel(FillableDocument(authoringAllowed = false))
        assertFalse(forbidden.state.value.formFields!!.authoringAllowed)
    }

    @Test
    fun aTapPlacesTheArmedFieldWhereItLanded() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        viewModel.armFormField(FormFieldTap.Place(NewFormField.Text))
        viewModel.tapFormField(1, AnnotationPoint(20.0, 100.0))
        advanceUntilIdle()

        val rect = placedFieldRect(NewFormField.Text, AnnotationPoint(20.0, 100.0), page)
        assertEquals(listOf(Triple(1, NewFormField.Text, rect)), document.placements)
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(fieldPlacedStatus(NewFormField.Text), state.status)
        assertEquals("the new field gets its row", 4, state.formFields!!.fields.size)
        assertNull("one tap, one field", state.formFields!!.armed)
    }

    @Test
    fun aPlacedFieldIsDrawnOnThePage() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        val refreshes = document.previewRefreshes
        viewModel.armFormField(FormFieldTap.Place(NewFormField.Checkbox))
        viewModel.tapFormField(0, AnnotationPoint(50.0, 50.0))
        advanceUntilIdle()

        assertTrue("only the renderer paints a field, so the preview must be rebuilt", document.previewRefreshes > refreshes)
    }

    @Test
    fun aTapWithNothingArmedPlacesNothing() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        viewModel.tapFormField(0, AnnotationPoint(50.0, 50.0))
        advanceUntilIdle()

        assertTrue(document.placements.isEmpty())
    }

    @Test
    fun aDocumentThatForbidsCreatingFieldsCannotBeArmed() = runTest {
        val document = FillableDocument(authoringAllowed = false)
        val viewModel = openedWithPanel(document)
        viewModel.armFormField(FormFieldTap.Place(NewFormField.Text))
        viewModel.tapFormField(0, AnnotationPoint(50.0, 50.0))
        advanceUntilIdle()

        assertNull(viewModel.state.value.formFields!!.armed)
        assertTrue(document.placements.isEmpty())
    }

    @Test
    fun aRefusedPlacementSaysWhyAndLeavesTheDocumentClean() = runTest {
        val document = FillableDocument(authoringRefusal = PdfCoreError.Failed("This document does not permit creating form fields."))
        val viewModel = openedWithPanel(document)
        viewModel.armFormField(FormFieldTap.Place(NewFormField.Text))
        viewModel.tapFormField(0, AnnotationPoint(50.0, 50.0))
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals("This document does not permit creating form fields.", state.status)
        assertFalse(state.isDirty)
        assertEquals(3, state.formFields!!.fields.size)
    }

    @Test
    fun aTapMovesTheArmedFieldKeepingItsSize() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        viewModel.armFormField(FormFieldTap.Move(1, 0))
        viewModel.tapFormField(0, AnnotationPoint(30.0, 80.0))
        advanceUntilIdle()

        val rect = movedFieldRect(AnnotationRect(10.0, 150.0, 60.0, 20.0), AnnotationPoint(30.0, 80.0), page)
        assertEquals(listOf(1L to rect), document.moves)
        val state = viewModel.state.value
        assertEquals(FIELD_MOVED, state.status)
        assertEquals(rect, state.formFields!!.fields.first { it.id == 1L }.rect)
        assertNull(state.formFields!!.armed)
        assertTrue(state.isDirty)
    }

    @Test
    fun aFieldCannotBeMovedToAnotherPage() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        viewModel.armFormField(FormFieldTap.Move(1, 0))
        viewModel.tapFormField(1, AnnotationPoint(30.0, 80.0))
        advanceUntilIdle()

        assertTrue(document.moves.isEmpty())
        assertEquals("Tap page 1 to move Name.", viewModel.state.value.status)
        assertEquals("still armed: the reader can tap the right page", FormFieldTap.Move(1, 0), viewModel.state.value.formFields!!.armed)
    }

    @Test
    fun undoingAPlacementTakesItsRowAway() = runTest {
        val viewModel = openedWithPanel(FillableDocument())
        viewModel.armFormField(FormFieldTap.Place(NewFormField.Dropdown))
        viewModel.tapFormField(0, AnnotationPoint(50.0, 50.0))
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(3, viewModel.state.value.formFields!!.fields.size)
    }

    @Test
    fun anUndoKeepsTheArmedTapWhileItsFieldStillExists() = runTest {
        val viewModel = openedWithPanel(FillableDocument())
        viewModel.armFormField(FormFieldTap.Place(NewFormField.Checkbox))
        viewModel.tapFormField(0, AnnotationPoint(50.0, 50.0))
        advanceUntilIdle()
        viewModel.armFormField(FormFieldTap.Move(1, 0))
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(FormFieldTap.Move(1, 0), viewModel.state.value.formFields!!.armed)
    }

    @Test
    fun anUndoThatTakesTheArmedFieldAwayDisarms() = runTest {
        val viewModel = openedWithPanel(FillableDocument())
        viewModel.armFormField(FormFieldTap.Place(NewFormField.Checkbox))
        viewModel.tapFormField(0, AnnotationPoint(50.0, 50.0))
        advanceUntilIdle()
        viewModel.armFormField(FormFieldTap.Move(4, 0))
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertNull(viewModel.state.value.formFields!!.armed)
    }

    @Test
    fun armingTakesThePageTapFromTheOtherModes() = runTest {
        val viewModel = openedWithPanel(FillableDocument())
        viewModel.setAnnotationTool(AnnotationTool.Shape)
        viewModel.openContentEdit()
        viewModel.armFormField(FormFieldTap.Place(NewFormField.Text))

        val state = viewModel.state.value
        assertNull(state.contentEdit)
        assertEquals(AnnotationTool.Pointer, state.activeAnnotationTool)
        assertEquals(fieldPlacementPrompt(NewFormField.Text), state.status)
    }

    @Test
    fun theOtherModesDisarm() = runTest {
        val viewModel = openedWithPanel(FillableDocument())
        viewModel.armFormField(FormFieldTap.Place(NewFormField.Text))
        viewModel.setAnnotationTool(AnnotationTool.Shape)
        assertNull(viewModel.state.value.formFields!!.armed)

        viewModel.armFormField(FormFieldTap.Place(NewFormField.Text))
        viewModel.openContentEdit()
        assertNull(viewModel.state.value.formFields!!.armed)
    }

    @Test
    fun armingNothingDisarms() = runTest {
        val viewModel = openedWithPanel(FillableDocument())
        viewModel.armFormField(FormFieldTap.Place(NewFormField.Text))
        viewModel.armFormField(null)

        assertNull(viewModel.state.value.formFields!!.armed)
    }

    @Test
    fun aNewSizeResizesTheFieldKeepingItsTopLeftCorner() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        val refreshes = document.previewRefreshes
        viewModel.resizeFormField(viewModel.state.value.documentId, 1, 80.0, 30.0)
        advanceUntilIdle()

        val rect = AnnotationRect(10.0, 140.0, 80.0, 30.0)
        assertEquals(listOf(1L to rect), document.resizes)
        assertTrue("a resize is not a move", document.moves.isEmpty())
        val state = viewModel.state.value
        assertEquals(FIELD_RESIZED, state.status)
        assertEquals(rect, state.formFields!!.fields.first { it.id == 1L }.rect)
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertTrue("only the renderer paints a field, so the preview must be rebuilt", document.previewRefreshes > refreshes)
    }

    @Test
    fun theSameSizeAgainIsNoEdit() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        viewModel.resizeFormField(viewModel.state.value.documentId, 1, 60.0, 20.0)
        advanceUntilIdle()

        assertTrue(document.resizes.isEmpty())
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun aSizeThatIsNotAPositiveNumberIsRefusedBeforeTheCore() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        viewModel.resizeFormField(viewModel.state.value.documentId, 1, Double.NaN, 30.0)
        advanceUntilIdle()

        assertTrue(document.resizes.isEmpty())
        assertEquals(FIELD_SIZE_INVALID, viewModel.state.value.status)
    }

    @Test
    fun aDocumentThatForbidsChangingFieldsIsNotResized() = runTest {
        val document = FillableDocument(authoringAllowed = false)
        val viewModel = openedWithPanel(document)
        viewModel.resizeFormField(viewModel.state.value.documentId, 1, 80.0, 30.0)
        advanceUntilIdle()

        assertTrue(document.resizes.isEmpty())
    }

    @Test
    fun aResizeFromARowOfAnotherDocumentIsDropped() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        viewModel.resizeFormField(viewModel.state.value.documentId + 1, 1, 80.0, 30.0)
        advanceUntilIdle()

        assertTrue(document.resizes.isEmpty())
    }

    @Test
    fun aRefusedResizeSaysWhyAndLeavesTheDocumentClean() = runTest {
        val document = FillableDocument(authoringRefusal = PdfCoreError.Failed("This document does not permit resizing form fields."))
        val viewModel = openedWithPanel(document)
        viewModel.resizeFormField(viewModel.state.value.documentId, 1, 80.0, 30.0)
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals("This document does not permit resizing form fields.", state.status)
        assertFalse(state.isDirty)
        assertEquals(AnnotationRect(10.0, 150.0, 60.0, 20.0), state.formFields!!.fields.first { it.id == 1L }.rect)
    }

    @Test
    fun deletingAFieldRemovesItsRowAndRedrawsThePage() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        val refreshes = document.previewRefreshes
        viewModel.deleteFormField(viewModel.state.value.documentId, 1)
        advanceUntilIdle()

        assertEquals(listOf(1L), document.removals)
        val state = viewModel.state.value
        assertEquals(FIELD_DELETED, state.status)
        assertTrue(state.formFields!!.fields.none { it.id == 1L })
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertTrue("only the renderer paints a field, so the preview must be rebuilt", document.previewRefreshes > refreshes)
    }

    @Test
    fun deletingTheFieldArmedForAMoveDisarmsIt() = runTest {
        val viewModel = openedWithPanel(FillableDocument())
        viewModel.armFormField(FormFieldTap.Move(1, 0))
        viewModel.deleteFormField(viewModel.state.value.documentId, 1)
        advanceUntilIdle()

        assertNull(viewModel.state.value.formFields!!.armed)
    }

    @Test
    fun aDocumentThatForbidsChangingFieldsKeepsItsFields() = runTest {
        val document = FillableDocument(authoringAllowed = false)
        val viewModel = openedWithPanel(document)
        viewModel.deleteFormField(viewModel.state.value.documentId, 1)
        advanceUntilIdle()

        assertTrue(document.removals.isEmpty())
    }

    @Test
    fun aDeleteFromARowOfAnotherDocumentIsDropped() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        viewModel.deleteFormField(viewModel.state.value.documentId + 1, 1)
        advanceUntilIdle()

        assertTrue(document.removals.isEmpty())
    }

    @Test
    fun aRefusedDeleteSaysWhyAndKeepsTheRow() = runTest {
        val document = FillableDocument(authoringRefusal = PdfCoreError.Failed("This document does not permit deleting form fields."))
        val viewModel = openedWithPanel(document)
        viewModel.deleteFormField(viewModel.state.value.documentId, 1)
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals("This document does not permit deleting form fields.", state.status)
        assertFalse(state.isDirty)
        assertTrue(state.formFields!!.fields.any { it.id == 1L })
    }

    @Test
    fun undoingADeleteBringsTheFieldBack() = runTest {
        val viewModel = openedWithPanel(FillableDocument())
        viewModel.deleteFormField(viewModel.state.value.documentId, 1)
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertTrue(viewModel.state.value.formFields!!.fields.any { it.id == 1L })
    }

    @Test
    fun undoingAResizePutsTheOldSizeBack() = runTest {
        val viewModel = openedWithPanel(FillableDocument())
        viewModel.resizeFormField(viewModel.state.value.documentId, 1, 80.0, 30.0)
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(AnnotationRect(10.0, 150.0, 60.0, 20.0), viewModel.state.value.formFields!!.fields.first { it.id == 1L }.rect)
    }

    private suspend fun TestScope.openedWithPanel(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        viewModel.openFormFields()
        advanceUntilIdle()
        return viewModel
    }
}
