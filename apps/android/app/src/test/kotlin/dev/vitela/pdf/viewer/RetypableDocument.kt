package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.ContentFontKind
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.ContentTextRun
import dev.vitela.pdf.core.PageContent
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit

/**
 * A two-page document whose pages paint text runs: "Hello" and "World" on
 * page 0 — the second in a composite font the core substitutes — and
 * "Page two" on page 1. Page 0 also paints one image, clear of both runs.
 * Every retype, text delete, resize, move and image delete is undoable, like the core's.
 *
 * Like the core, a re-read reports a retyped run under its original id with
 * the text it now shows — a resized or moved image with the box it now fills — and
 * rendering draws the *preview*: the runs as they stood at open and again only
 * after [refreshPreview]. [drawn] records, per render, the page and the texts
 * it showed.
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
    private var images = listOf(
        ContentImage(30, 0, AnnotationRect(10.0, 20.0, 30.0, 30.0), "Im1"),
    )
    private val undoable = ArrayDeque<Pair<Content, Content>>()
    private val redoable = ArrayDeque<Pair<Content, Content>>()
    /** Each retype the core accepted: the run as the shell sent it, and the new text. */
    val retypes = mutableListOf<Pair<ContentTextRun, String>>()
    /** Each text delete the core accepted: the run as the shell sent it. */
    val textDeletes = mutableListOf<ContentTextRun>()
    /** Each resize the core accepted: the image as the shell sent it, and the box it now fills. */
    val resizes = mutableListOf<Pair<ContentImage, AnnotationRect>>()
    /** Each move the core accepted: the image as the shell sent it, and the box it now fills. */
    val moves = mutableListOf<Pair<ContentImage, AnnotationRect>>()
    /** Each delete the core accepted: the image as the shell sent it. */
    val deletes = mutableListOf<ContentImage>()
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

    override fun pageContent(pageIndex: Int): PdfCoreResult<PageContent> {
        reads += pageIndex
        return PdfCoreResult.Success(PageContent(runs.filter { it.pageIndex == pageIndex }, images.filter { it.pageIndex == pageIndex }))
    }

    override fun retypeTextRun(run: ContentTextRun, text: String): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        retypes += run to text
        record { runs = runs.map { if (it.id == run.id) it.copy(text = text) else it } }
        return PdfCoreResult.Success(Unit)
    }

    override fun removeTextRun(run: ContentTextRun): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        textDeletes += run
        record { runs = runs.filter { it.id != run.id } }
        return PdfCoreResult.Success(Unit)
    }

    override fun resizeImage(image: ContentImage, to: AnnotationRect): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        resizes += image to to
        record { images = images.map { if (it.id == image.id) it.copy(bounds = to) else it } }
        return PdfCoreResult.Success(Unit)
    }

    override fun moveImage(image: ContentImage, to: AnnotationRect): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        moves += image to to
        record { images = images.map { if (it.id == image.id) it.copy(bounds = to) else it } }
        return PdfCoreResult.Success(Unit)
    }

    override fun removeImage(image: ContentImage): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        deletes += image
        record { images = images.filter { it.id != image.id } }
        return PdfCoreResult.Success(Unit)
    }

    override fun undoAnnotations(): PdfCoreResult<Boolean> {
        val step = undoable.removeLastOrNull() ?: return PdfCoreResult.Success(false)
        restore(step.first)
        redoable.addLast(step)
        return PdfCoreResult.Success(true)
    }

    override fun redoAnnotations(): PdfCoreResult<Boolean> {
        val step = redoable.removeLastOrNull() ?: return PdfCoreResult.Success(false)
        restore(step.second)
        undoable.addLast(step)
        return PdfCoreResult.Success(true)
    }

    private fun record(edit: () -> Unit) {
        val before = Content(runs, images)
        edit()
        undoable.addLast(before to Content(runs, images))
        redoable.clear()
    }

    private fun restore(content: Content) {
        runs = content.runs
        images = content.images
    }

    private data class Content(val runs: List<ContentTextRun>, val images: List<ContentImage>)

    override fun close() = Unit
}
