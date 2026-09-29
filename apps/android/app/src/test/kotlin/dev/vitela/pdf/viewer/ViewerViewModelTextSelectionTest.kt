package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationEdit
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.PageCharacters
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit
import dev.vitela.pdf.core.TextRect
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * T-086: long-press and drag selects text in reading order, with every caret,
 * rect and character supplied by the core's `PageCharacters`.
 */
class ViewerViewModelTextSelectionTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun aDragPublishesTheCoresRectsAndText() = runTest {
        val document = SelectableDocument()
        val viewModel = opened(document)

        viewModel.beginTextSelection(0, AnnotationPoint(10.0, 705.0))
        viewModel.extendTextSelection(AnnotationPoint(40.0, 705.0))

        val selection = viewModel.state.first { it.textSelection != null }.textSelection
        assertEquals(TextSelection(0, listOf(TextRect(10.0, 700.0, 30.0, 12.0)), "ell"), selection)
    }

    @Test
    fun endingTheDragKeepsTheSelectionAndReleasesTheCharacters() = runTest {
        val document = SelectableDocument()
        val viewModel = opened(document)
        viewModel.beginTextSelection(0, AnnotationPoint(10.0, 705.0))
        viewModel.extendTextSelection(AnnotationPoint(40.0, 705.0))
        viewModel.state.first { it.textSelection != null }

        viewModel.endTextSelection()

        assertEquals("ell", viewModel.state.value.textSelection?.text)
        assertTrue(document.loaded.single().closed)
    }

    @Test
    fun aNewDragReplacesTheOldSelectionAndReleasesItsCharacters() = runTest {
        val document = SelectableDocument()
        val viewModel = opened(document)
        viewModel.beginTextSelection(0, AnnotationPoint(10.0, 705.0))
        viewModel.extendTextSelection(AnnotationPoint(40.0, 705.0))
        viewModel.state.first { it.textSelection != null }

        viewModel.beginTextSelection(0, AnnotationPoint(0.0, 705.0))

        assertNull(viewModel.state.value.textSelection)
        assertTrue(document.loaded.first().closed)
    }

    @Test
    fun aTapOnEmptyPageClearsTheSelection() = runTest {
        val viewModel = opened(SelectableDocument())
        viewModel.beginTextSelection(0, AnnotationPoint(10.0, 705.0))
        viewModel.extendTextSelection(AnnotationPoint(40.0, 705.0))
        viewModel.endTextSelection()
        viewModel.state.first { it.textSelection != null }

        val tap = AnnotationPoint(300.0, 300.0)
        viewModel.handlePageGesture(0, tap, tap, emptyList(), 4.0)

        assertNull(viewModel.state.value.textSelection)
    }

    @Test
    fun aRefusedExtractionExplainsItselfAndSelectsNothing() = runTest {
        val viewModel = opened(SelectableDocument(refusal = PdfCoreError.Failed("Text extraction is not permitted.")))

        viewModel.beginTextSelection(0, AnnotationPoint(10.0, 705.0))
        viewModel.extendTextSelection(AnnotationPoint(40.0, 705.0))

        assertEquals("Text extraction is not permitted.", viewModel.state.first { it.status.startsWith("Text extraction") }.status)
        assertNull(viewModel.state.value.textSelection)
    }

    @Test
    fun highlightingASelectionMarksTheCoresLineRects() = runTest {
        val document = SelectableDocument()
        val viewModel = opened(document)
        viewModel.beginTextSelection(0, AnnotationPoint(10.0, 705.0))
        viewModel.extendTextSelection(AnnotationPoint(40.0, 705.0))
        viewModel.endTextSelection()
        viewModel.state.first { it.textSelection != null }

        viewModel.setAnnotationTool(AnnotationTool.Highlight)
        viewModel.state.first { it.isDirty }

        val added = document.edits.filterIsInstance<AnnotationEdit.Add>().single().annotation
        assertEquals(AnnotationKind.Highlight, added.kind)
        assertEquals(AnnotationRect(10.0, 700.0, 30.0, 12.0), added.rect)
        assertNull(viewModel.state.value.textSelection)
    }

    @Test
    fun replacingTheDocumentDropsTheSelectionAndItsCharacters() = runTest {
        val document = SelectableDocument()
        val viewModel = opened(document)
        viewModel.beginTextSelection(0, AnnotationPoint(10.0, 705.0))
        viewModel.extendTextSelection(AnnotationPoint(40.0, 705.0))
        viewModel.state.first { it.textSelection != null }

        viewModel.open("b.pdf", byteArrayOf(2))
        viewModel.state.first { it.title == "b.pdf" && !it.isLoading }

        assertNull(viewModel.state.value.textSelection)
        assertTrue(document.loaded.single().closed)
    }

    private suspend fun opened(document: SelectableDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(object : PdfCore {
            override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> =
                PdfCoreResult.Success(if (bytes.contentEquals(byteArrayOf(1))) document else SelectableDocument())
        })
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.annotationEditingAllowed }
        return viewModel
    }
}

private class SelectableDocument(private val refusal: PdfCoreError? = null) : PdfDocument {
    val loaded = mutableListOf<LineCharacters>()
    val edits = mutableListOf<AnnotationEdit>()

    override val pageCount = 1
    override val pageSizes = listOf(PageSize(612.0, 792.0))
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun annotations(): PdfCoreResult<AnnotationSnapshot> = PdfCoreResult.Success(AnnotationSnapshot(emptyList(), editingAllowed = true, canUndo = false, canRedo = false))
    override fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> {
        edits += edit
        return PdfCoreResult.Success(Unit)
    }

    override fun pageCharacters(pageIndex: Int): PdfCoreResult<PageCharacters> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        return PdfCoreResult.Success(LineCharacters("Hello").also { loaded += it })
    }

    override fun close() = Unit
}
