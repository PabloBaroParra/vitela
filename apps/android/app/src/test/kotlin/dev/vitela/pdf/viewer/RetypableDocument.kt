package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.ContentFontKind
import dev.vitela.pdf.core.ContentTextRun
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit

/**
 * A two-page document whose pages paint text runs: "Hello" and "World" on
 * page 0 — the second in a composite font the core substitutes — and
 * "Page two" on page 1. Every retype is undoable, like the core's.
 *
 * Like the core, a re-read reports a retyped run under its original id with
 * the text it now shows, and rendering draws the *preview*: the runs as they
 * stood at open and again only after [refreshPreview]. [drawn] records, per
 * render, the page and the texts it showed.
 */
internal class RetypableDocument(
    private val editingAllowed: Boolean = true,
    private val refusal: PdfCoreError? = null,
) : PdfDocument {
    private var runs = listOf(
        ContentTextRun(10, 0, AnnotationRect(10.0, 150.0, 40.0, 12.0), "F1", ContentFontKind.Standard14, "Hello"),
        ContentTextRun(11, 0, AnnotationRect(10.0, 120.0, 40.0, 12.0), "F2", ContentFontKind.EmbeddedComposite, "World"),
        ContentTextRun(20, 1, AnnotationRect(10.0, 150.0, 60.0, 12.0), "F1", ContentFontKind.Standard14, "Page two"),
    )
    private val undoable = ArrayDeque<Pair<List<ContentTextRun>, List<ContentTextRun>>>()
    private val redoable = ArrayDeque<Pair<List<ContentTextRun>, List<ContentTextRun>>>()
    /** Each retype the core accepted: the run as the shell sent it, and the new text. */
    val retypes = mutableListOf<Pair<ContentTextRun, String>>()
    val drawn = mutableListOf<Pair<Int, List<String>>>()
    val reads = mutableListOf<Int>()
    private var preview = runs

    override val pageCount: Int = 2
    override val pageSizes: List<PageSize> = List(2) { PageSize(100.0, 200.0) }

    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> {
        drawn += pageIndex to preview.filter { it.pageIndex == pageIndex }.map { it.text }
        return PdfCoreResult.Success(RenderedPage(0, 0, 0, ByteArray(0)))
    }

    override fun refreshPreview(): PdfCoreResult<Unit> {
        preview = runs
        return PdfCoreResult.Success(Unit)
    }

    override fun search(query: String) = PdfCoreResult.Success(listOf(SearchHit(0, "x")))

    override fun annotations() = PdfCoreResult.Success(AnnotationSnapshot(emptyList(), true, undoable.isNotEmpty(), redoable.isNotEmpty()))

    override fun contentEditingAllowed(): Boolean = editingAllowed

    override fun pageTextRuns(pageIndex: Int): PdfCoreResult<List<ContentTextRun>> {
        reads += pageIndex
        return PdfCoreResult.Success(runs.filter { it.pageIndex == pageIndex })
    }

    override fun retypeTextRun(run: ContentTextRun, text: String): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        retypes += run to text
        val before = runs
        runs = runs.map { if (it.id == run.id) it.copy(text = text) else it }
        undoable.addLast(before to runs)
        redoable.clear()
        return PdfCoreResult.Success(Unit)
    }

    override fun undoAnnotations(): PdfCoreResult<Boolean> {
        val step = undoable.removeLastOrNull() ?: return PdfCoreResult.Success(false)
        runs = step.first
        redoable.addLast(step)
        return PdfCoreResult.Success(true)
    }

    override fun redoAnnotations(): PdfCoreResult<Boolean> {
        val step = redoable.removeLastOrNull() ?: return PdfCoreResult.Success(false)
        runs = step.second
        undoable.addLast(step)
        return PdfCoreResult.Success(true)
    }

    override fun close() = Unit
}
