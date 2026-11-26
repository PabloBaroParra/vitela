package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.TextRunLook
import dev.vitela.pdf.core.TextRunLookState
import dev.vitela.pdf.core.TextStyleToggle
import dev.vitela.pdf.text.UiText
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
 * Bold / italic / underline toggles on existing text in Edit content: the
 * toggles follow the first run of the target — the open editor's block, or a
 * selection's text — a press evens a mixed target out as one core call and one
 * undo step, and a style that is the run's own font with no plain sibling stays
 * on and says why. The Windows shell's TextStyleParityTests, through the ViewModel.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelTextStyleTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    // RetypableDocument: page 1 paints the two-run block "Two runs" (runs 21, 22); page 0 "Hello" (10) and "World" (11), an image and two paths.
    private val onRuns = AnnotationPoint(60.0, 105.0)
    private val wholePage = AnnotationRect(0.0, 0.0, 100.0, 200.0)
    private val onImage = AnnotationRect(15.0, 25.0, 5.0, 5.0)

    private val plain = TextRunLookState(TextRunLook())
    private val bold = TextRunLook(bold = true)

    private fun ViewerViewModel.editor() = state.value.contentEdit?.editor

    @Test
    fun openingALineReadsItsTogglesFromTheCore() = runTest {
        val document = StylableDocument(mapOf(21L to TextRunLookState(TextRunLook(italic = true, underline = true)), 22L to TextRunLookState(bold, canRemoveBold = false)))
        val viewModel = editing(document)

        val style = viewModel.editor()?.style
        assertEquals("the first run's look, queued restyles included", TextRunLook(italic = true, underline = true), style?.look)
        assertFalse(style!!.isFixed(TextStyleToggle.Bold))
        assertEquals(listOf(21L, 22L), document.lookReads)
    }

    @Test
    fun aRunWhoseLookCannotBeReadLeavesTheTogglesOff() = runTest {
        val viewModel = editing(StylableDocument(mapOf(22L to null)))

        assertTrue(viewModel.editor() != null)
        assertNull(viewModel.editor()?.style)
    }

    @Test
    fun aPressEvensAMixedLineOutInOneCall() = runTest {
        val document = StylableDocument(mapOf(21L to plain, 22L to TextRunLookState(bold)))
        val viewModel = editing(document)
        viewModel.toggleEditorTextStyle(viewModel.state.value.documentId, TextStyleToggle.Bold, "Two runs")
        advanceUntilIdle()

        val restyles = document.restyles.single()
        assertEquals("the run already bold is left out", listOf(21L), restyles.map { it.run.id })
        assertEquals(TextRunLook(), restyles.single().from)
        assertEquals(bold, restyles.single().to)
        val state = viewModel.state.value
        assertTrue(state.isDirty)
        assertTrue("it enters history", state.canUndoAnnotations)
        assertEquals(TEXT_RESTYLED, state.status)
    }

    @Test
    fun theEditorStaysOpenOverTheRestyledLineWithWhatWasTyped() = runTest {
        val document = StylableDocument()
        val viewModel = editing(document)
        viewModel.toggleEditorTextStyle(viewModel.state.value.documentId, TextStyleToggle.Underline, "Typed meanwhile")
        advanceUntilIdle()

        val editor = viewModel.editor()
        assertEquals(listOf(21L, 22L), editor?.block?.runs?.map { it.id })
        assertEquals("the field keeps what was typed", "Typed meanwhile", editor?.text)
        assertTrue("the toggle shows pressed now", editor?.style?.look?.underline == true)
        assertNull(editor?.error)

        // A second press stacks on the first, read from the restyled runs.
        viewModel.toggleEditorTextStyle(viewModel.state.value.documentId, TextStyleToggle.Italic, "Typed meanwhile")
        advanceUntilIdle()
        assertEquals(TextRunLook(underline = true), document.restyles.last().first().from)
        assertEquals(TextRunLook(italic = true, underline = true), document.restyles.last().first().to)
    }

    @Test
    fun aFixedBoldIsRefusedBeforeTheCoreAndSaysWhy() = runTest {
        val document = StylableDocument(mapOf(21L to TextRunLookState(bold, canRemoveBold = false), 22L to TextRunLookState(bold)))
        val viewModel = editing(document)
        viewModel.toggleEditorTextStyle(viewModel.state.value.documentId, TextStyleToggle.Bold, "Two runs")
        advanceUntilIdle()

        assertEquals(fixedStyleReason(TextStyleToggle.Bold), viewModel.editor()?.error)
        assertTrue("nothing reaches the core", document.restyles.isEmpty())
        assertFalse(viewModel.state.value.isDirty)

        viewModel.toggleEditorTextStyle(viewModel.state.value.documentId, TextStyleToggle.Italic, "Two runs")
        advanceUntilIdle()
        assertEquals("italic still goes on over a fixed bold", listOf(21L, 22L), document.restyles.single().map { it.run.id })
    }

    @Test
    fun theCoresRefusalReachesTheReader() = runTest {
        val document = StylableDocument()
        val refusal = PdfCoreError.Failed(UiText.Verbatim("This text has an unsaved edit — save the document before editing it again."))
        document.restyleRefusal = refusal
        val viewModel = editing(document)
        viewModel.toggleEditorTextStyle(viewModel.state.value.documentId, TextStyleToggle.Underline, "Two runs")
        advanceUntilIdle()

        assertEquals(refusal.text, viewModel.editor()?.error)
        assertFalse("a refused restyle changes nothing", viewModel.state.value.isDirty)
    }

    @Test
    fun aRestyleIsOneUndoStep() = runTest {
        val document = StylableDocument()
        val viewModel = editing(document)
        viewModel.toggleEditorTextStyle(viewModel.state.value.documentId, TextStyleToggle.Bold, "Two runs")
        advanceUntilIdle()
        viewModel.undoAnnotations()
        advanceUntilIdle()

        assertEquals(TextRunLook(), document.lookOf(21L)?.look)
        assertEquals(TextRunLook(), document.lookOf(22L)?.look)
        assertFalse("nothing else is queued", viewModel.state.value.canUndoAnnotations)
    }

    @Test
    fun aDialogBuiltForAnotherDocumentRestylesNothing() = runTest {
        val document = StylableDocument()
        val viewModel = editing(document)
        viewModel.toggleEditorTextStyle(viewModel.state.value.documentId + 1, TextStyleToggle.Bold, "Two runs")
        advanceUntilIdle()

        assertTrue(document.restyles.isEmpty())
    }

    @Test
    fun aSelectionsTextShowsTogglesAndAPressRestylesEveryBlockAsOneStep() = runTest {
        val document = StylableDocument()
        val viewModel = armed(document)
        viewModel.marqueeContent(0, wholePage)
        advanceUntilIdle()

        val mode = viewModel.state.value.contentEdit!!
        assertEquals(TextRunLook(), mode.selectionButtons?.look)

        viewModel.toggleSelectionTextStyle(TextStyleToggle.Underline)
        advanceUntilIdle()

        assertEquals("one call for both blocks", listOf(listOf(10L, 11L)), document.restyles.map { call -> call.map { it.run.id } })
        val after = viewModel.state.value.contentEdit!!
        assertEquals("the selection is kept", listOf(10L, 11L), after.selection?.blocks?.map { it.id })
        assertTrue("its toggles show the new look", after.selectionButtons?.look?.underline == true)
        assertEquals(TEXT_RESTYLED, viewModel.state.value.status)
    }

    @Test
    fun aSelectionWithNoTextShowsNoToggles() = runTest {
        val document = StylableDocument()
        val viewModel = armed(document)
        viewModel.marqueeContent(0, onImage)
        advanceUntilIdle()

        assertEquals(1, viewModel.state.value.contentEdit?.selection?.count)
        assertNull(viewModel.state.value.contentEdit?.selectionButtons)
        viewModel.toggleSelectionTextStyle(TextStyleToggle.Bold)
        advanceUntilIdle()
        assertTrue(document.restyles.isEmpty())
    }

    @Test
    fun aFixedBoldInASelectionIsReportedInTheStatus() = runTest {
        val document = StylableDocument(mapOf(10L to TextRunLookState(bold, canRemoveBold = false)))
        val viewModel = armed(document)
        viewModel.marqueeContent(0, wholePage)
        advanceUntilIdle()
        viewModel.toggleSelectionTextStyle(TextStyleToggle.Bold)
        advanceUntilIdle()

        assertTrue(document.restyles.isEmpty())
        assertEquals(fixedStyleReason(TextStyleToggle.Bold), viewModel.state.value.status)
        assertTrue("the selection stays", viewModel.state.value.contentEdit?.selection != null)
    }

    private suspend fun TestScope.editing(document: PdfDocument): ViewerViewModel = armed(document).also {
        it.tapContent(1, onRuns, reach = 0.0)
        advanceUntilIdle()
    }

    private suspend fun TestScope.armed(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        viewModel.openContentEdit()
        advanceUntilIdle()
        return viewModel
    }
}
