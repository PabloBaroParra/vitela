package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.PageEdit
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit

/**
 * A document whose pages are identified by their width (100 + the id the page
 * was born with), so a test can read the current order straight from
 * [pageSizes]. Every edit is undoable, like the core's.
 *
 * Rendering draws the *preview*, like the core's `render_page`: a snapshot of
 * the pages taken at open and again only by [refreshPreview]. [drawn] records
 * which page id each render actually showed.
 */
internal class OrganizableDocument(
    pageCount: Int = 4,
    private val refusal: PdfCoreError? = null,
    private val previewFailure: PdfCoreError? = null,
) : PdfDocument {
    private class Page(val id: Int, var turned: Boolean = false)

    private var pages = MutableList(pageCount) { Page(it) }

    /** A blank page is born with the next id, so it too reads back by its width; landscape is a turned one. */
    private var nextId = pageCount
    private val undoable = ArrayDeque<Pair<() -> Unit, () -> Unit>>()
    private val redoable = ArrayDeque<Pair<() -> Unit, () -> Unit>>()
    val edits = mutableListOf<PageEdit>()
    val rendered = mutableListOf<Pair<Int, Int>>()
    val drawn = mutableListOf<Int>()
    private var preview = pages.map { it.id }
    var previewRefreshes = 0
        private set
    var pageSizeReads = 0
        private set

    override val pageCount: Int get() = pages.size
    override val pageSizes: List<PageSize>
        get() {
            pageSizeReads++
            return pages.map { if (it.turned) PageSize(200.0, 100.0 + it.id) else PageSize(100.0 + it.id, 200.0) }
        }

    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> {
        rendered += pageIndex to dpi
        drawn += preview[pageIndex]
        // Not a valid bitmap: the conversion is Android's, and returns null on it.
        return PdfCoreResult.Success(RenderedPage(0, 0, 0, ByteArray(0)))
    }

    override fun refreshPreview(): PdfCoreResult<Unit> {
        previewRefreshes++
        previewFailure?.let { return PdfCoreResult.Failure(it) }
        preview = pages.map { it.id }
        return PdfCoreResult.Success(Unit)
    }

    override fun search(query: String) = PdfCoreResult.Success(listOf(SearchHit(1, "x")))

    override fun annotations() = PdfCoreResult.Success(AnnotationSnapshot(emptyList(), true, undoable.isNotEmpty(), redoable.isNotEmpty()))

    override fun applyPageEdit(edit: PageEdit): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        edits += edit
        val before = pages.toList()
        val after = when (edit) {
            is PageEdit.Move -> before.toMutableList().also { it.add(edit.to, it.removeAt(edit.from)) }
            is PageEdit.Remove -> before.toMutableList().also { it.removeAt(edit.pageIndex) }
            is PageEdit.Rotate -> before.toMutableList().also { it[edit.pageIndex].turned = !it[edit.pageIndex].turned }
            is PageEdit.InsertBlank -> before.toMutableList().also { it.add(edit.index, Page(nextId++, turned = edit.landscape)) }
        }
        pages = after.toMutableList()
        val undo = { pages = before.toMutableList(); if (edit is PageEdit.Rotate) before[edit.pageIndex].turned = !before[edit.pageIndex].turned }
        val redo = { pages = after.toMutableList(); if (edit is PageEdit.Rotate) after[edit.pageIndex].turned = !after[edit.pageIndex].turned }
        undoable.addLast(undo to redo)
        redoable.clear()
        return PdfCoreResult.Success(Unit)
    }

    /** An edit that is not a page edit, so undo has something to undo. */
    fun stackAnAnnotationUndo() {
        undoable.addLast(({ }) to ({ }))
    }

    override fun undoAnnotations(): PdfCoreResult<Boolean> {
        val step = undoable.removeLastOrNull() ?: return PdfCoreResult.Success(false)
        step.first()
        redoable.addLast(step)
        return PdfCoreResult.Success(true)
    }

    override fun redoAnnotations(): PdfCoreResult<Boolean> {
        val step = redoable.removeLastOrNull() ?: return PdfCoreResult.Success(false)
        step.second()
        undoable.addLast(step)
        return PdfCoreResult.Success(true)
    }

    override fun close() = Unit
}

internal class OrganizeQueueCore(vararg documents: PdfDocument) : PdfCore {
    private val queue = ArrayDeque(documents.toList())
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> = PdfCoreResult.Success(queue.removeFirst())
}
