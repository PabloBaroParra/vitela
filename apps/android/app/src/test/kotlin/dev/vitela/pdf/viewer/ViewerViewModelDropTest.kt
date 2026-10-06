package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * T-092: an image dropped on a page lands where it was let go — no armed tool,
 * no second tap. The drop point is the stamp's top-left corner, as on Linux
 * and Windows.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelDropTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun aDroppedImageIsStampedAtTheDropPoint() = runTest {
        val document = DropDocument()
        val viewModel = opened(document)

        viewModel.dropImageStamp(1, AnnotationPoint(30.0, 200.0), byteArrayOf(5))
        advanceUntilIdle()

        val stamp = viewModel.state.value.annotations.single()
        assertEquals(1, stamp.pageIndex)
        assertEquals(AnnotationRect(30.0, 180.0, 40.0, 20.0), stamp.rect)
        assertArrayEquals(byteArrayOf(5), viewModel.state.value.stampImages[stamp.id])
        assertEquals(IMAGE_DROPPED_PLACED, viewModel.state.value.status)
        assertTrue(viewModel.state.value.isDirty)
    }

    @Test
    fun aDropLeavesTheArmedToolAlone() = runTest {
        val viewModel = opened(DropDocument())
        viewModel.setAnnotationTool(AnnotationTool.Highlight)

        viewModel.dropImageStamp(0, AnnotationPoint(30.0, 200.0), byteArrayOf(5))
        advanceUntilIdle()

        assertEquals(AnnotationTool.Highlight, viewModel.state.value.activeAnnotationTool)
    }

    @Test
    fun aDocumentThatForbidsAnnotatingRefusesTheDrop() = runTest {
        val document = DropDocument(editable = false)
        val viewModel = opened(document)

        viewModel.dropImageStamp(0, AnnotationPoint(30.0, 200.0), byteArrayOf(5))
        advanceUntilIdle()

        assertTrue(document.stamps.isEmpty())
        assertEquals(DROP_NOT_ANNOTATABLE, viewModel.state.value.status)
    }

    @Test
    fun aRefusedDropSaysWhy() = runTest {
        val viewModel = opened(DropDocument())
        viewModel.refuseDrop(DROP_UNSUPPORTED)

        assertEquals(DROP_UNSUPPORTED, viewModel.state.value.status)
    }

    private suspend fun TestScope.opened(document: PdfDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}

/** Accepts image stamps — unless the document forbids annotating. */
private class DropDocument(
    private val editable: Boolean = true,
    private val base: PdfDocument = RetypableDocument(),
) : PdfDocument by base {
    val stamps = mutableListOf<Annotation>()
    private var nextId = 100L

    override fun annotations() = PdfCoreResult.Success(AnnotationSnapshot(stamps.toList(), editable, stamps.isNotEmpty(), false))

    override fun insertImageStamp(pageIndex: Int, imageBytes: ByteArray, rect: AnnotationRect): PdfCoreResult<Unit> {
        stamps += Annotation(nextId++, pageIndex, AnnotationKind.Stamp, rect, null)
        return PdfCoreResult.Success(Unit)
    }
}
