package dev.vitela.pdf.viewer

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

/**
 * T-091: Paste arms the image stamp with the clipboard's image, the way
 * choosing an image does; the next tap places it. A clipboard with no image
 * says so and arms nothing.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelClipboardPasteTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun aPastedImageArmsTheStampTool() = runTest {
        val viewModel = opened()
        viewModel.pasteImageStamp(byteArrayOf(1, 2, 3))

        assertEquals(AnnotationTool.Stamp, viewModel.state.value.activeAnnotationTool)
        assertEquals(PASTE_STAMP_PROMPT, viewModel.state.value.status)
    }

    @Test
    fun aClipboardWithoutAnImageArmsNothing() = runTest {
        val viewModel = opened()
        viewModel.setAnnotationTool(AnnotationTool.Highlight)
        viewModel.refusePaste()

        assertEquals(AnnotationTool.Highlight, viewModel.state.value.activeAnnotationTool)
        assertEquals(CLIPBOARD_HAS_NO_IMAGE, viewModel.state.value.status)
    }

    private suspend fun TestScope.opened(): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(RetypableDocument()))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}
