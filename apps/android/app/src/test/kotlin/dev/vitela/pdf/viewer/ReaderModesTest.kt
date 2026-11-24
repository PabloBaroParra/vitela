package dev.vitela.pdf.viewer

import dev.vitela.pdf.R
import dev.vitela.pdf.resources.StringResources
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ReaderModesTest {
    @Test
    fun stayingInTheSameModeUndoesNothing() {
        val state = ViewerState(activeAnnotationTool = AnnotationTool.Ink, contentEdit = ContentEditState())

        assertEquals(ModeExit.None, modeExit(ReaderMode.Edit, ReaderMode.Edit, state))
    }

    @Test
    fun leavingEditDisarmsTheToolAndClosesContentEditing() {
        val state = ViewerState(activeAnnotationTool = AnnotationTool.Highlight, contentEdit = ContentEditState())

        val exit = modeExit(ReaderMode.Edit, ReaderMode.Read, state)

        assertTrue(exit.disarmTool)
        assertTrue(exit.closeContentEdit)
        assertFalse(exit.closeFormFields)
    }

    @Test
    fun leavingEditWithNothingArmedUndoesNothing() {
        assertEquals(ModeExit.None, modeExit(ReaderMode.Edit, ReaderMode.Sign, ViewerState()))
    }

    @Test
    fun leavingSignClosesTheFormFieldsPanel() {
        val state = ViewerState(formFields = FormFieldsState())

        assertEquals(ModeExit(disarmTool = false, closeContentEdit = false, closeFormFields = true), modeExit(ReaderMode.Sign, ReaderMode.Edit, state))
    }

    @Test
    fun leavingSignDisarmsADrawnSignatureWaitingForItsTap() {
        val state = ViewerState(activeAnnotationTool = AnnotationTool.Stamp)

        assertEquals(ModeExit(disarmTool = true, closeContentEdit = false, closeFormFields = false), modeExit(ReaderMode.Sign, ReaderMode.Read, state))
    }

    @Test
    fun theTabsAreReadEditAndSign() {
        assertEquals(listOf("Read", "Edit", "Sign"), ReaderMode.entries.map { StringResources.english.text(it.label) })
        assertEquals(listOf("Leer", "Editar", "Firmar"), ReaderMode.entries.map { StringResources.spanish.text(it.label) })
    }

    @Test
    fun theModeBarHidesWhileThePageGridIsOpen() {
        assertTrue(modeBarVisible(ViewerState(pageCount = 3)))
        assertFalse(modeBarVisible(ViewerState(pageCount = 3, organize = OrganizeState())))
    }

    @Test
    fun theHomeScreenShowsUntilADocumentHasPages() {
        assertTrue(showsHome(ViewerState()))
        assertFalse(showsHome(ViewerState(pageCount = 1)))
    }

    private fun pageLabel(state: ViewerState) = StringResources.english.text(R.string.reader_page_of, pageNumber(state), state.pageCount)

    @Test
    fun thePagePillCountsFromOne() {
        assertEquals("1 of 4", pageLabel(ViewerState(pageCount = 4, pageIndex = 0)))
        assertEquals("4 of 4", pageLabel(ViewerState(pageCount = 4, pageIndex = 3)))
    }
}
