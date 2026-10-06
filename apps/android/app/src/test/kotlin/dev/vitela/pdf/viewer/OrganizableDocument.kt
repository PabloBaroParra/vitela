package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.BlockSource
import dev.vitela.pdf.core.DocumentBlock
import dev.vitela.pdf.core.BatchImportReport
import dev.vitela.pdf.core.PageEdit
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.PreparedImport
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
    private val annotationList: List<Annotation> = emptyList(),
    private val annotationEditingAllowed: Boolean = true,
) : PdfDocument {
    private class Page(val id: Int, var turned: Boolean = false, val source: BlockSource = BlockSource.Base)

    private var pages = MutableList(pageCount) { Page(it) }

    /** A blank page is born with the next id, so it too reads back by its width; landscape is a turned one. */
    private var nextId = pageCount
    private var nextSourceId = 40L
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

    override fun annotations() = PdfCoreResult.Success(AnnotationSnapshot(annotationList, annotationEditingAllowed, undoable.isNotEmpty(), redoable.isNotEmpty()))

    override fun applyPageEdit(edit: PageEdit): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        edits += edit
        val before = pages.toList()
        val after = when (edit) {
            is PageEdit.Move -> before.toMutableList().also { list ->
                val run = List(edit.count) { list.removeAt(edit.from) }
                list.addAll(edit.to, run)
            }
            is PageEdit.Remove -> before.toMutableList().also { list -> repeat(edit.count) { list.removeAt(edit.pageIndex) } }
            is PageEdit.Rotate -> before.toMutableList().also { turn(it, edit) }
            is PageEdit.InsertBlank -> before.toMutableList().also { it.add(edit.index, Page(nextId++, turned = edit.landscape, source = BlockSource.Blank)) }
        }
        pages = after.toMutableList()
        val undo = { pages = before.toMutableList(); if (edit is PageEdit.Rotate) turn(before, edit) }
        val redo = { pages = after.toMutableList(); if (edit is PageEdit.Rotate) turn(after, edit) }
        undoable.addLast(undo to redo)
        redoable.clear()
        return PdfCoreResult.Success(Unit)
    }

    private fun turn(pages: List<Page>, edit: PageEdit.Rotate) {
        for (index in edit.pageIndex until edit.pageIndex + edit.count) pages[index].turned = !pages[index].turned
    }

    /** One block per run of pages from the same source, like the core's `derive_blocks`. */
    override fun documentBlocks(): PdfCoreResult<List<DocumentBlock>> {
        val runs = mutableListOf<Pair<BlockSource, IntRange>>()
        pages.forEachIndexed { index, page ->
            val last = runs.lastOrNull()
            if (last != null && last.first == page.source) runs[runs.lastIndex] = last.first to (last.second.first..index)
            else runs += page.source to (index..index)
        }
        val split = runs.groupingBy { it.first }.eachCount().filterValues { it > 1 }.keys
        val seen = mutableMapOf<BlockSource, Int>()
        return PdfCoreResult.Success(
            runs.map { (source, range) ->
                val part = if (source in split) seen.merge(source, 1, Int::plus) else null
                DocumentBlock(source, part, range.first, range.last - range.first + 1)
            },
        )
    }

    /** Every batch import asked of the core: the position it was asked at, and how many sources it carried. */
    val imports = mutableListOf<Pair<Int, Int>>()

    /** Set to refuse every batch the way the core's own gates do, before anything is added. */
    var importRefusal: String? = null

    /** Adds every [FakePreparedImport]'s pages as new ids, one block per source and ONE undo step for the batch. */
    override fun importPrepared(sources: List<PreparedImport>, index: Int): PdfCoreResult<BatchImportReport> {
        imports += index to sources.size
        importRefusal?.let { return PdfCoreResult.Failure(PdfCoreError.Failed(it)) }
        val fakes = sources.map { it as FakePreparedImport }
        check(fakes.none { it.closed }) { "a closed source reached the core" }
        val before = pages.toList()
        val sourceIds = fakes.map { nextSourceId++ }
        val added = fakes.zip(sourceIds).flatMap { (source, id) -> List(source.pageCount) { Page(nextId++, source = BlockSource.Imported(id)) } }
        val after = before.toMutableList().also { it.addAll(index, added) }
        pages = after
        undoable.addLast(({ pages = before.toMutableList() }) to ({ pages = after.toMutableList() }))
        redoable.clear()
        return PdfCoreResult.Success(BatchImportReport(added.size, sourceIds))
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

internal const val FAKE_PLAIN: Byte = 0
internal const val FAKE_LOCKED: Byte = 1
internal const val FAKE_REFUSED: Byte = 2
internal const val FAKE_WARNS: Byte = 3
internal const val FAKE_PASSWORD = "fixture-secret"

/** The bytes [OrganizeQueueCore.prepareImport] reads as a PDF of [pages] pages of [kind]. */
internal fun fakePdf(pages: Int, kind: Byte = FAKE_PLAIN) = byteArrayOf(kind, pages.toByte())

/** A source [OrganizeQueueCore.prepareImport] opened; [closed] once the shell let go of it. */
internal class FakePreparedImport(override val pageCount: Int, override val warnings: List<String>) : PreparedImport {
    var closed = false
        private set

    override fun close() {
        closed = true
    }
}

internal class OrganizeQueueCore(vararg documents: PdfDocument) : PdfCore {
    private val queue = ArrayDeque(documents.toList())
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> = PdfCoreResult.Success(queue.removeFirst())

    /** Every source prepared, in order — to check that each one is closed. */
    val prepared = mutableListOf<FakePreparedImport>()

    /** Every preparation asked of the core: the bytes' page count and the password it came with. */
    val preparations = mutableListOf<Pair<Int, String?>>()

    /** Reads a [fakePdf]: the source's own gates, its password, and what its pages would lose. */
    override fun prepareImport(bytes: ByteArray, password: String?): PdfCoreResult<PreparedImport> {
        preparations += bytes[1].toInt() to password
        when (bytes[0]) {
            FAKE_LOCKED -> if (password != FAKE_PASSWORD) return PdfCoreResult.Failure(if (password == null) PdfCoreError.PasswordRequired else PdfCoreError.WrongPassword)
            FAKE_REFUSED -> return PdfCoreResult.Failure(PdfCoreError.Failed("The PDF does not permit copying its pages."))
        }
        val warnings = if (bytes[0] == FAKE_WARNS) listOf("a form field was renamed") else emptyList()
        return PdfCoreResult.Success(FakePreparedImport(bytes[1].toInt(), warnings).also { prepared += it })
    }
}
