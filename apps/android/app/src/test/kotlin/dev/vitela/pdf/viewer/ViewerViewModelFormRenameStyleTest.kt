package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationColor
import dev.vitela.pdf.core.FieldFont
import dev.vitela.pdf.core.FieldTextStyle
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Renaming and restyling existing fields from the Form fields panel, each one undoable edit of the shared log. */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelFormRenameStyleTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    private val times = FieldTextStyle(FieldFont.TimesRoman, 14.0, AnnotationColor(0, 0, 200))

    @Test
    fun renamingAFieldRenamesItsRowAndRedrawsThePage() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        val refreshes = document.previewRefreshes
        viewModel.renameFormField(viewModel.state.value.documentId, 1, "Full name")
        advanceUntilIdle()

        assertEquals(listOf(1L to "Full name"), document.renames)
        val state = viewModel.state.value
        assertEquals(FIELD_RENAMED, state.status)
        assertEquals("Full name", state.formFields!!.fields.first { it.id == 1L }.name)
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertTrue(document.previewRefreshes > refreshes)
    }

    @Test
    fun aRenameIsTrimmedAndTheSameNameCostsNothing() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        viewModel.renameFormField(viewModel.state.value.documentId, 1, "  Name ")
        advanceUntilIdle()

        assertTrue(document.renames.isEmpty())
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun anEmptyNameIsRefusedBeforeTheCore() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        viewModel.renameFormField(viewModel.state.value.documentId, 1, "   ")
        advanceUntilIdle()

        assertTrue(document.renames.isEmpty())
        assertEquals(FIELD_NAME_EMPTY, viewModel.state.value.status)
    }

    @Test
    fun anotherFieldsNameIsRefusedBeforeTheCore() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        viewModel.renameFormField(viewModel.state.value.documentId, 1, "Agree")
        advanceUntilIdle()

        assertTrue(document.renames.isEmpty())
        assertEquals(fieldNameTaken("Agree"), viewModel.state.value.status)
    }

    @Test
    fun undoingARenamePutsTheOldNameBack() = runTest {
        val viewModel = openedWithPanel(FillableDocument())
        viewModel.renameFormField(viewModel.state.value.documentId, 1, "Full name")
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals("Name", viewModel.state.value.formFields!!.fields.first { it.id == 1L }.name)
    }

    @Test
    fun restylingAFieldChangesItsStyleAndRedrawsThePage() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        val refreshes = document.previewRefreshes
        viewModel.restyleFormField(viewModel.state.value.documentId, 1, times)
        advanceUntilIdle()

        assertEquals(listOf(1L to times), document.restyles)
        val state = viewModel.state.value
        assertEquals(FIELD_RESTYLED, state.status)
        assertEquals(times, state.formFields!!.fields.first { it.id == 1L }.style)
        assertTrue(state.canUndoAnnotations)
        assertTrue(document.previewRefreshes > refreshes)
    }

    @Test
    fun theStyleAFieldAlreadyHasCostsNothing() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        viewModel.restyleFormField(viewModel.state.value.documentId, 1, FieldTextStyle())
        advanceUntilIdle()

        assertTrue(document.restyles.isEmpty())
    }

    @Test
    fun aFontSizeOutsideTheRangeIsRefusedBeforeTheCore() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        val documentId = viewModel.state.value.documentId
        listOf(0.5, 73.0, Double.NaN).forEach { size ->
            viewModel.restyleFormField(documentId, 1, FieldTextStyle(sizePt = size))
            advanceUntilIdle()
            assertEquals(FIELD_FONT_SIZE_INVALID, viewModel.state.value.status)
        }

        assertTrue(document.restyles.isEmpty())
    }

    @Test
    fun undoingARestylePutsTheOldStyleBack() = runTest {
        val viewModel = openedWithPanel(FillableDocument())
        viewModel.restyleFormField(viewModel.state.value.documentId, 1, times)
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(FieldTextStyle(), viewModel.state.value.formFields!!.fields.first { it.id == 1L }.style)
    }

    @Test
    fun aDocumentThatForbidsChangingFieldsKeepsTheirNamesAndStyles() = runTest {
        val document = FillableDocument(authoringAllowed = false)
        val viewModel = openedWithPanel(document)
        val documentId = viewModel.state.value.documentId
        viewModel.renameFormField(documentId, 1, "Full name")
        viewModel.restyleFormField(documentId, 1, times)
        advanceUntilIdle()

        assertTrue(document.renames.isEmpty())
        assertTrue(document.restyles.isEmpty())
    }

    @Test
    fun anEditFromARowOfAnotherDocumentIsDropped() = runTest {
        val document = FillableDocument()
        val viewModel = openedWithPanel(document)
        val otherDocument = viewModel.state.value.documentId + 1
        viewModel.renameFormField(otherDocument, 1, "Full name")
        viewModel.restyleFormField(otherDocument, 1, times)
        advanceUntilIdle()

        assertTrue(document.renames.isEmpty())
        assertTrue(document.restyles.isEmpty())
    }

    @Test
    fun aRefusedRenameSaysWhyAndKeepsTheName() = runTest {
        val document = FillableDocument(authoringRefusal = PdfCoreError.Failed("That name is already used by another field."))
        val viewModel = openedWithPanel(document)
        viewModel.renameFormField(viewModel.state.value.documentId, 1, "Full name")
        advanceUntilIdle()

        val state = viewModel.state.value
        assertEquals("That name is already used by another field.", state.status)
        assertFalse(state.isDirty)
        assertEquals("Name", state.formFields!!.fields.first { it.id == 1L }.name)
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
