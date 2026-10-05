package dev.vitela.pdf.viewer

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
    fun leavingSignClosesTheFormFieldsPanelOnly() {
        val state = ViewerState(activeAnnotationTool = AnnotationTool.Ink, formFields = FormFieldsState())

        assertEquals(ModeExit(disarmTool = false, closeContentEdit = false, closeFormFields = true), modeExit(ReaderMode.Sign, ReaderMode.Edit, state))
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

    @Test
    fun thePagePillCountsFromOne() {
        assertEquals("1 of 4", pageLabel(ViewerState(pageCount = 4, pageIndex = 0)))
        assertEquals("4 of 4", pageLabel(ViewerState(pageCount = 4, pageIndex = 3)))
    }
}
