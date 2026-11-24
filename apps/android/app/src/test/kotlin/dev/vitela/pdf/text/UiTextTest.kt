package dev.vitela.pdf.text

import dev.vitela.pdf.R
import dev.vitela.pdf.core.CompressedCopy
import dev.vitela.pdf.core.NewFormField
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.viewer.compressReductionSummary
import dev.vitela.pdf.viewer.fieldNameTaken
import dev.vitela.pdf.viewer.fieldPlacedStatus
import dev.vitela.pdf.viewer.fieldPlacementPrompt
import dev.vitela.pdf.viewer.imageMovePrompt
import dev.vitela.pdf.viewer.importSummary
import dev.vitela.pdf.viewer.pastedMessage
import dev.vitela.pdf.viewer.pathMovePrompt
import dev.vitela.pdf.viewer.selectedMessage
import dev.vitela.pdf.viewer.textMovePrompt
import dev.vitela.pdf.viewer.userMessage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class UiTextTest {
    @Test
    fun sameTextIsEqualAndDifferentArgumentsAreNot() {
        assertEquals(UiText.Resource(R.string.status_search_match, 1, 2), UiText.Resource(R.string.status_search_match, 1, 2))
        assertNotEquals(UiText.Resource(R.string.status_search_match, 1, 2), UiText.Resource(R.string.status_search_match, 2, 2))
        assertEquals(UiText.Plural(R.plurals.status_extract_busy, 2, 2), UiText.Plural(R.plurals.status_extract_busy, 2, 2))
        assertEquals(UiText.Sentences(UiText.Verbatim("a")), UiText.Sentences(UiText.Verbatim("a")))
    }

    @Test
    fun theCountPicksTheForm() {
        assertEquals("Extracting 1 page...", UiText.Plural(R.plurals.status_extract_busy, 1, 1).english())
        assertEquals("Extracting 0 pages...", UiText.Plural(R.plurals.status_extract_busy, 0, 0).english())
        assertEquals("Extrayendo 2 páginas...", UiText.Plural(R.plurals.status_extract_busy, 2, 2).spanish())
    }

    @Test
    fun anArgumentIsWordedInTheSameLanguage() {
        assertEquals("1 página añadida de 2 PDF. Los cambios están pendientes de guardar.", importSummary(1, 2).spanish())
        assertEquals(
            "2,0 MB sin comprimir → 500 kB comprimido (75 % menos). Elige dónde guardarlo.",
            compressReductionSummary(CompressedCopy(ByteArray(0), 2_000_000, 500_000, 1_500_000, true, emptyList())).spanish(),
        )
    }

    /** The sentences with arguments no other test words, in both languages. */
    @Test
    fun everySentenceWithArgumentsIsWorded() {
        val sentences = listOf(
            imageMovePrompt(0) to "Drag the image, or tap page 1 where its top-left corner should go.",
            textMovePrompt(1) to "Drag the text, or tap page 2 where its top-left corner should go.",
            pathMovePrompt(2) to "Drag the shape, or tap page 3 where its top-left corner should go.",
            fieldNameTaken("Agree") to "\"Agree\" is already used by another field.",
            fieldPlacementPrompt(NewFormField.Text) to "Tap a page to place a text field.",
            fieldPlacementPrompt(NewFormField.RadioGroup) to "Tap a page to place a radio group.",
            fieldPlacementPrompt(NewFormField.Dropdown) to "Tap a page to place a dropdown.",
            fieldPlacedStatus(NewFormField.Text) to "Text field placed. Save to keep the change.",
            fieldPlacedStatus(NewFormField.Checkbox) to "Checkbox placed. Save to keep the change.",
            fieldPlacedStatus(NewFormField.RadioGroup) to "Radio group placed. Save to keep the change.",
            fieldPlacedStatus(NewFormField.Dropdown) to "Dropdown placed. Save to keep the change.",
            pastedMessage(1) to "Pasted 1 item — drag inside the dashed box to place them.",
            pastedMessage(2) to "Pasted 2 items — drag inside the dashed box to place them.",
            selectedMessage(2) to "2 items selected — drag inside the dashed box to move them, Delete removes them, tap an item to add or remove it.",
            UiText.Resource(R.string.status_search_match, 1, 4) to "Match 1 of 4.",
            UiText.Resource(R.string.status_export_busy, 3, 1, 2) to "Exporting page 3 (1 of 2)...",
            UiText.Resource(R.string.status_split_busy, 1, 2) to "Writing part 1 of 2...",
            UiText.Resource(R.string.status_import_password_waiting, "a.pdf") to "Waiting for the password for a.pdf.",
            UiText.Resource(R.string.status_open_recent_removed, "a.pdf") to "a.pdf is no longer available and was removed from Recent.",
        )
        sentences.forEach { (text, english) ->
            assertEquals(english, text.english())
            assertNotEquals(english, text.spanish())
        }
    }

    @Test
    fun argumentsAreNeverReadAsFormats() {
        assertEquals("Could not open 100%d.pdf.", UiText.Resource(R.string.status_open_recent_failed, "100%d.pdf").english())
    }

    @Test
    fun sentencesAreJoinedWhole() {
        val text = UiText.Sentences(UiText.Resource(R.string.status_sign_not_reopened), UiText.Verbatim("boom."))
        assertEquals("The signed PDF was written, but it could not be reopened. boom.", text.english())
    }

    @Test
    fun theCoresOwnSentenceIsKeptAndThePasswordOneIsTranslated() {
        assertEquals("boom", userMessage(PdfCoreError.Failed("boom")).spanish())
        assertEquals("Este documento requiere una contraseña.", userMessage(PdfCoreError.WrongPassword).spanish())
    }

    @Test
    fun aWrongNumberOfArgumentsIsCaught() {
        assertThrows(AssertionError::class.java) { UiText.Resource(R.string.status_search_match, 1).english() }
        assertThrows(AssertionError::class.java) { UiText.Resource(R.string.status_search_none, 1).english() }
        assertThrows(AssertionError::class.java) { UiText.Plural(R.plurals.status_extract_busy, 1).english() }
    }
}
