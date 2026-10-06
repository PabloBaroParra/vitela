package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.NewFormField
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
 * T-088: **Draw signature** opens a pad; using the drawing arms the image
 * stamp with its PNG, the way Paste does, and the next tap places it.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelDrawnSignatureTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun usingTheDrawingClosesThePadAndArmsTheStamp() = runTest {
        val viewModel = opened()
        viewModel.openSignaturePad()
        assertTrue(viewModel.state.value.signaturePadOpen)

        viewModel.useDrawnSignature(viewModel.state.value.documentId, byteArrayOf(1, 2, 3))

        assertFalse(viewModel.state.value.signaturePadOpen)
        assertEquals(AnnotationTool.Stamp, viewModel.state.value.activeAnnotationTool)
        assertEquals(SIGNATURE_STAMP_PROMPT, viewModel.state.value.status)
    }

    @Test
    fun theSignatureTakesTheNextTapFromAnArmedFormField() = runTest {
        val viewModel = opened(FillableDocument())
        viewModel.openFormFields()
        advanceUntilIdle()
        viewModel.armFormField(FormFieldTap.Place(NewFormField.Text))
        viewModel.openSignaturePad()

        viewModel.useDrawnSignature(viewModel.state.value.documentId, byteArrayOf(1))

        assertNull("a page tap goes to the armed field first, so it must let go", viewModel.state.value.formFields!!.armed)
        assertEquals(AnnotationTool.Stamp, viewModel.state.value.activeAnnotationTool)
    }

    @Test
    fun cancellingArmsNothing() = runTest {
        val viewModel = opened()
        viewModel.openSignaturePad()
        viewModel.closeSignaturePad()

        assertFalse(viewModel.state.value.signaturePadOpen)
        assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
    }

    @Test
    fun aSignatureRenderedAfterThePadWasCancelledArmsNothing() = runTest {
        val viewModel = opened()
        viewModel.openSignaturePad()
        viewModel.closeSignaturePad()

        viewModel.useDrawnSignature(viewModel.state.value.documentId, byteArrayOf(1))

        assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
    }

    @Test
    fun aSignatureThatCouldNotBeRenderedArmsNothing() = runTest {
        val viewModel = opened()
        viewModel.openSignaturePad()
        viewModel.useDrawnSignature(viewModel.state.value.documentId, null)

        assertFalse(viewModel.state.value.signaturePadOpen)
        assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
        assertEquals(SIGNATURE_UNRENDERABLE, viewModel.state.value.status)
    }

    @Test
    fun aSignatureDrawnForAReplacedDocumentArmsNothing() = runTest {
        val viewModel = opened()
        val drawnFor = viewModel.state.value.documentId
        viewModel.openSignaturePad()
        viewModel.open("b.pdf", byteArrayOf(2))
        viewModel.state.first { it.title == "b.pdf" && !it.isLoading }
        advanceUntilIdle()
        assertFalse(viewModel.state.value.signaturePadOpen)

        viewModel.useDrawnSignature(drawnFor, byteArrayOf(1))

        assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
    }

    @Test
    fun aDocumentThatForbidsAnnotatingNeverOpensThePad() = runTest {
        val viewModel = opened(RestrictedDocument())
        assertFalse(viewModel.state.value.annotationEditingAllowed)

        viewModel.openSignaturePad()

        assertFalse(viewModel.state.value.signaturePadOpen)
    }

    private suspend fun TestScope.opened(document: PdfDocument = RetypableDocument()): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document, RetypableDocument()))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}

private class RestrictedDocument(private val base: PdfDocument = RetypableDocument()) : PdfDocument by base {
    override fun annotations() = PdfCoreResult.Success(AnnotationSnapshot(emptyList(), false, false, false))
}
