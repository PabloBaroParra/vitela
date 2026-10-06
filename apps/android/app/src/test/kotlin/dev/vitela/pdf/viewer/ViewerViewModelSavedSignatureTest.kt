package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * A remembered signature: drawn once with **Remember** ticked, it is offered
 * the next time **Draw signature** is tapped — to use as is, to replace with a
 * new drawing, or to delete.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelSavedSignatureTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun withNothingRememberedThePadOpens() = runTest {
        val viewModel = opened(MemorySignatureStore())

        viewModel.openSignaturePad()
        advanceUntilIdle()

        assertTrue(viewModel.state.value.signaturePadOpen)
        assertNull(viewModel.state.value.signatureChoice)
    }

    @Test
    fun aRememberedSignatureIsOfferedInsteadOfThePad() = runTest {
        val viewModel = opened(MemorySignatureStore(byteArrayOf(4, 2)))

        viewModel.openSignaturePad()
        advanceUntilIdle()

        assertFalse(viewModel.state.value.signaturePadOpen)
        assertArrayEquals(byteArrayOf(4, 2), viewModel.state.value.signatureChoice)
    }

    @Test
    fun usingTheRememberedSignatureArmsItWithoutDrawing() = runTest {
        val viewModel = opened(MemorySignatureStore(byteArrayOf(4, 2)))
        viewModel.openSignaturePad()
        advanceUntilIdle()

        viewModel.useSavedSignature(viewModel.state.value.documentId)

        assertNull(viewModel.state.value.signatureChoice)
        assertEquals(AnnotationTool.Stamp, viewModel.state.value.activeAnnotationTool)
        assertEquals(SIGNATURE_STAMP_PROMPT, viewModel.state.value.status)
    }

    @Test
    fun aRememberedSignatureOfferedForAReplacedDocumentArmsNothing() = runTest {
        val viewModel = opened(MemorySignatureStore(byteArrayOf(4, 2)))
        val offeredFor = viewModel.state.value.documentId
        viewModel.openSignaturePad()
        advanceUntilIdle()
        viewModel.open("b.pdf", byteArrayOf(2))
        viewModel.state.first { it.title == "b.pdf" && !it.isLoading }
        advanceUntilIdle()

        viewModel.useSavedSignature(offeredFor)

        assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
    }

    @Test
    fun drawingANewOneOpensThePad() = runTest {
        val viewModel = opened(MemorySignatureStore(byteArrayOf(4, 2)))
        viewModel.openSignaturePad()
        advanceUntilIdle()

        viewModel.drawNewSignature()

        assertNull(viewModel.state.value.signatureChoice)
        assertTrue(viewModel.state.value.signaturePadOpen)
    }

    @Test
    fun aDrawingUsedWithRememberReplacesTheSavedOne() = runTest {
        val store = MemorySignatureStore(byteArrayOf(4, 2))
        val viewModel = opened(store)
        viewModel.openSignaturePad()
        advanceUntilIdle()
        viewModel.drawNewSignature()

        viewModel.useDrawnSignature(viewModel.state.value.documentId, byteArrayOf(7), remember = true)
        advanceUntilIdle()

        assertArrayEquals(byteArrayOf(7), store.png)
        assertEquals(AnnotationTool.Stamp, viewModel.state.value.activeAnnotationTool)
    }

    @Test
    fun aDrawingUsedWithoutRememberIsUsedOnceAndKeepsTheSavedOne() = runTest {
        val store = MemorySignatureStore(byteArrayOf(4, 2))
        val viewModel = opened(store)
        viewModel.openSignaturePad()
        advanceUntilIdle()
        viewModel.drawNewSignature()

        viewModel.useDrawnSignature(viewModel.state.value.documentId, byteArrayOf(7), remember = false)
        advanceUntilIdle()

        assertArrayEquals(byteArrayOf(4, 2), store.png)
        assertEquals(AnnotationTool.Stamp, viewModel.state.value.activeAnnotationTool)
    }

    @Test
    fun aSignatureThatCouldNotBeRememberedIsStillUsed() = runTest {
        val viewModel = opened(MemorySignatureStore(failsToSave = true))
        viewModel.openSignaturePad()
        advanceUntilIdle()

        viewModel.useDrawnSignature(viewModel.state.value.documentId, byteArrayOf(7), remember = true)
        advanceUntilIdle()

        assertEquals(AnnotationTool.Stamp, viewModel.state.value.activeAnnotationTool)
        assertEquals(SIGNATURE_NOT_REMEMBERED, viewModel.state.value.status)
    }

    @Test
    fun deletingForgetsTheSignatureAndArmsNothing() = runTest {
        val store = MemorySignatureStore(byteArrayOf(4, 2))
        val viewModel = opened(store)
        viewModel.openSignaturePad()
        advanceUntilIdle()

        viewModel.deleteSavedSignature()
        advanceUntilIdle()

        assertNull(store.png)
        assertNull(viewModel.state.value.signatureChoice)
        assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
        assertEquals(SIGNATURE_FORGOTTEN, viewModel.state.value.status)
    }

    @Test
    fun cancellingTheOfferKeepsTheSignatureAndArmsNothing() = runTest {
        val store = MemorySignatureStore(byteArrayOf(4, 2))
        val viewModel = opened(store)
        viewModel.openSignaturePad()
        advanceUntilIdle()

        viewModel.closeSignatureChoice()

        assertNull(viewModel.state.value.signatureChoice)
        assertArrayEquals(byteArrayOf(4, 2), store.png)
        assertEquals(AnnotationTool.Pointer, viewModel.state.value.activeAnnotationTool)
    }

    private suspend fun TestScope.opened(store: SignatureStore, document: PdfDocument = RetypableDocument()): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document, RetypableDocument()), store)
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}

internal class MemorySignatureStore(var png: ByteArray? = null, private val failsToSave: Boolean = false) : SignatureStore {
    override fun load() = png
    override fun save(png: ByteArray): Boolean {
        if (failsToSave) return false
        this.png = png
        return true
    }
    override fun delete() {
        png = null
    }
}
