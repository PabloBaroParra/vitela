package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.FormFieldValue
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Filling in the fields a document already has, each fill one undoable edit of the shared log. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelFormFillTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun openingThePanelListsTheDocumentsFields() = runTest {
        val viewModel = openedWith(FillableDocument())
        viewModel.openFormFields()
        advanceUntilIdle()

        val panel = viewModel.state.value.formFields
        assertNotNull(panel)
        panel!!
        assertEquals(listOf("Name", "Agree", "Country"), panel.fields.map { it.name })
        assertTrue(panel.fillAllowed)
        assertTrue(panel.loaded)
    }

    @Test
    fun aFillReachesTheCoreAndMakesTheDocumentDirty() = runTest {
        val document = FillableDocument()
        val viewModel = openedWith(document)
        viewModel.openFormFields()
        advanceUntilIdle()
        viewModel.fillFormField(viewModel.state.value.documentId, 2, FormFieldValue.Checked(true))
        advanceUntilIdle()

        assertEquals(listOf(2L to FormFieldValue.Checked(true)), document.fills)
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertEquals(FORM_FILLED, state.status)
        assertEquals(FormFieldValue.Checked(true), state.formFields?.fields?.first { it.id == 2L }?.value)
    }

    @Test
    fun aFilledValueIsDrawnOnThePage() = runTest {
        val document = FillableDocument()
        val viewModel = openedWith(document)
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        advanceUntilIdle()
        viewModel.openFormFields()
        advanceUntilIdle()
        viewModel.fillFormField(viewModel.state.value.documentId, 1, FormFieldValue.Text("Grace"))
        advanceUntilIdle()

        val lastOfPageZero = document.drawn.last { it.first == 0 }.second
        assertEquals("rendering reads the preview, so it must be rebuilt to show the value", FormFieldValue.Text("Grace"), lastOfPageZero[1L])
    }

    @Test
    fun aFillKeepsTheSearchHits() = runTest {
        val viewModel = openedWith(FillableDocument())
        viewModel.search("x")
        advanceUntilIdle()
        viewModel.openFormFields()
        advanceUntilIdle()
        viewModel.fillFormField(viewModel.state.value.documentId, 2, FormFieldValue.Checked(true))
        advanceUntilIdle()

        assertEquals("a fill changes no page's position", 1, viewModel.state.value.searchHits.size)
    }

    @Test
    fun anUnchangedValueQueuesNothing() = runTest {
        val document = FillableDocument()
        val viewModel = openedWith(document)
        viewModel.openFormFields()
        advanceUntilIdle()
        viewModel.fillFormField(viewModel.state.value.documentId, 1, FormFieldValue.Text("Ada"))
        advanceUntilIdle()

        assertTrue(document.fills.isEmpty())
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun theSameValueCommittedTwiceQueuesOneEdit() = runTest {
        val document = FillableDocument()
        val viewModel = openedWith(document)
        viewModel.openFormFields()
        advanceUntilIdle()
        val documentId = viewModel.state.value.documentId
        // A text row commits on Done and again when it loses focus.
        viewModel.fillFormField(documentId, 1, FormFieldValue.Text("Grace"))
        viewModel.fillFormField(documentId, 1, FormFieldValue.Text("Grace"))
        advanceUntilIdle()

        assertEquals(1, document.fills.size)
    }

    @Test
    fun aDocumentThatForbidsFillingQueuesNothing() = runTest {
        val document = FillableDocument(fillAllowed = false)
        val viewModel = openedWith(document)
        viewModel.openFormFields()
        advanceUntilIdle()
        viewModel.fillFormField(viewModel.state.value.documentId, 2, FormFieldValue.Checked(true))
        advanceUntilIdle()

        assertTrue(document.fills.isEmpty())
        assertFalse(viewModel.state.value.formFields!!.fillAllowed)
    }

    @Test
    fun aRefusedFillPutsTheCoresValueBackAndSaysWhy() = runTest {
        val document = FillableDocument(refusal = PdfCoreError.Failed("That value does not fit this field."))
        val viewModel = openedWith(document)
        viewModel.openFormFields()
        advanceUntilIdle()
        viewModel.fillFormField(viewModel.state.value.documentId, 1, FormFieldValue.Text("Grace"))
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals(FormFieldValue.Text("Ada"), state.formFields?.fields?.first { it.id == 1L }?.value)
        assertEquals("That value does not fit this field.", state.status)
        assertFalse(state.isDirty)
    }

    @Test
    fun undoingAFillRedrawsThePageAndThePanel() = runTest {
        val document = FillableDocument()
        val viewModel = openedWith(document)
        viewModel.onReaderPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        viewModel.openFormFields()
        advanceUntilIdle()
        viewModel.fillFormField(viewModel.state.value.documentId, 1, FormFieldValue.Text("Grace"))
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(FormFieldValue.Text("Ada"), viewModel.state.value.formFields?.fields?.first { it.id == 1L }?.value)
        assertEquals(FormFieldValue.Text("Ada"), document.drawn.last { it.first == 0 }.second[1L])

        viewModel.redoAnnotations()
        advanceUntilIdle()

        assertEquals(FormFieldValue.Text("Grace"), viewModel.state.value.formFields?.fields?.first { it.id == 1L }?.value)
        assertEquals(FormFieldValue.Text("Grace"), document.drawn.last { it.first == 0 }.second[1L])
    }

    @Test
    fun anUndoBeforeAnyFillLeavesThePreviewAlone() = runTest {
        val document = FillableDocument()
        val viewModel = openedWith(document)
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(0, document.previewRefreshes)
    }

    @Test
    fun aCommitForAReplacedDocumentIsDropped() = runTest {
        val first = FillableDocument()
        val second = FillableDocument()
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(first, second))
        viewModel.open("a.pdf", byteArrayOf(1))
        advanceUntilIdle()
        val staleId = viewModel.state.value.documentId
        viewModel.open("b.pdf", byteArrayOf(2))
        advanceUntilIdle()
        viewModel.openFormFields()
        advanceUntilIdle()
        // A text row losing focus as the panel is torn down commits late.
        viewModel.fillFormField(staleId, 1, FormFieldValue.Text("Grace"))
        advanceUntilIdle()

        assertTrue(first.fills.isEmpty())
        assertTrue(second.fills.isEmpty())
    }

    @Test
    fun replacingTheDocumentClosesThePanel() = runTest {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(FillableDocument(), FillableDocument()))
        viewModel.open("a.pdf", byteArrayOf(1))
        advanceUntilIdle()
        viewModel.openFormFields()
        advanceUntilIdle()
        viewModel.open("b.pdf", byteArrayOf(2))
        advanceUntilIdle()

        assertNull(viewModel.state.value.formFields)
    }

    @Test
    fun organizingClosesThePanelAndThePanelWaitsForTheReader() = runTest {
        val viewModel = openedWith(FillableDocument())
        viewModel.openFormFields()
        advanceUntilIdle()
        viewModel.openOrganize()

        assertNull(viewModel.state.value.formFields)

        viewModel.openFormFields()
        advanceUntilIdle()

        assertNull("the grid hides the pages a fill would redraw", viewModel.state.value.formFields)
    }

    @Test
    fun closingThePanelForgetsItsFields() = runTest {
        val viewModel = openedWith(FillableDocument())
        viewModel.openFormFields()
        advanceUntilIdle()
        viewModel.closeFormFields()

        assertNull(viewModel.state.value.formFields)
    }

    private suspend fun TestScope.openedWith(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
