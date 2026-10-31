package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.ContentFontKind
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.ContentPath
import dev.vitela.pdf.core.ContentTextBlock
import dev.vitela.pdf.core.ContentTextRun
import dev.vitela.pdf.core.PageContent
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PathPaint
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit

/**
 * A two-page document whose pages paint text runs: "Hello" and "World" on
 * page 0 — the second in a composite font the core substitutes — and
 * "Page two" on page 1, which also paints one line in two runs, "Two" and
 * "runs", a word gap apart. Page 0 also paints one image, clear of both runs.
 * Runs are read as blocks the way the core groups them, simply: runs on one page,
 * one baseline, one font kind and height, no more than 5 points apart.
 * Every block retype, delete or move, insert, resize, move, image delete, image replacement and path move, reshape or delete is
 * undoable, like the core's. An inserted image is placed the way the core
 * places one: 40 by 20 points, its top-left corner on the anchor.
 *
 * Like the core, a re-read reports a retyped run under its original id with
 * the text it now shows — a moved run, a resized or moved image with the box it now fills — and
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
        ContentTextRun(21, 1, AnnotationRect(10.0, 100.0, 30.0, 12.0), "F1", ContentFontKind.Standard14, "Two"),
        ContentTextRun(22, 1, AnnotationRect(42.0, 100.0, 40.0, 12.0), "F1", ContentFontKind.Standard14, "runs"),
    )
    private var images = listOf(
        ContentImage(30, 0, AnnotationRect(10.0, 20.0, 30.0, 30.0), "Im1"),
    )
    private var paths = listOf(
        ContentPath(40, 0, AnnotationRect(10.0, 80.0, 60.0, 0.0), PathPaint.Stroke),
        ContentPath(41, 0, AnnotationRect(60.0, 90.0, 30.0, 20.0), PathPaint.Fill),
    )
    private val undoable = ArrayDeque<Pair<Content, Content>>()
    private val redoable = ArrayDeque<Pair<Content, Content>>()
    /** Each block retype the core accepted: the block as the shell sent it, and the new text. */
    val retypes = mutableListOf<Pair<ContentTextBlock, String>>()
    /** Each block delete the core accepted: the block as the shell sent it. */
    val textDeletes = mutableListOf<ContentTextBlock>()
    /** Each block move the core accepted: the block as the shell sent it, and the offset sent. */
    val textMoves = mutableListOf<Triple<ContentTextBlock, Double, Double>>()
    /** Each resize the core accepted: the image as the shell sent it, and the box it now fills. */
    val resizes = mutableListOf<Pair<ContentImage, AnnotationRect>>()
    /** Each move the core accepted: the image as the shell sent it, and the box it now fills. */
    val moves = mutableListOf<Pair<ContentImage, AnnotationRect>>()
    /** Each delete the core accepted: the image as the shell sent it. */
    val deletes = mutableListOf<ContentImage>()
    /** Each path move the core accepted: the path as the shell sent it, and the box sent. */
    val pathMoves = mutableListOf<Pair<ContentPath, AnnotationRect>>()
    /** Each path reshape the core accepted: the path as the shell sent it, and the box it now fills. */
    val pathResizes = mutableListOf<Pair<ContentPath, AnnotationRect>>()
    /** Each path delete the core accepted: the path as the shell sent it. */
    val pathDeletes = mutableListOf<ContentPath>()
    /** Each image whose original the core recovered before a picker opened. */
    val prepared = mutableListOf<ContentImage>()
    /** Each source replacement the core accepted: the image as the shell sent it, and the new bytes. */
    val replaces = mutableListOf<Pair<ContentImage, ByteArray>>()
    /** Each text insert the core accepted: the page, the text and the box sent. */
    val inserts = mutableListOf<Triple<Int, String, AnnotationRect>>()
    /** Each image insert the core accepted: the page, the bytes and the box sent. */
    val imageInserts = mutableListOf<Triple<Int, ByteArray, AnnotationRect>>()
    private var nextId = 100L
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
        val page = runs.filter { it.pageIndex == pageIndex }
        return PdfCoreResult.Success(PageContent(page, images.filter { it.pageIndex == pageIndex }, paths.filter { it.pageIndex == pageIndex }, blocksOf(page)))
    }

    private fun blocksOf(page: List<ContentTextRun>): List<ContentTextBlock> {
        val lines = mutableListOf<MutableList<ContentTextRun>>()
        for (run in page.sortedWith(compareByDescending<ContentTextRun> { it.bounds.y }.thenBy { it.bounds.x })) {
            val line = lines.lastOrNull()?.takeIf { it.last().joins(run) }
            if (line != null) line += run else lines += mutableListOf(run)
        }
        return lines.map { line ->
            val left = line.minOf { it.bounds.x }
            val right = line.maxOf { it.bounds.x + it.bounds.width }
            val first = line.first().bounds
            ContentTextBlock(line, AnnotationRect(left, first.y, right - left, first.height), line.joinToString(" ") { it.text })
        }
    }

    private fun ContentTextRun.joins(next: ContentTextRun) =
        bounds.y == next.bounds.y && bounds.height == next.bounds.height && fontKind == next.fontKind &&
            next.bounds.x - (bounds.x + bounds.width) in 0.0..5.0

    // The first run shows all the text and the others go, like the core's block retype.
    override fun retypeTextBlock(block: ContentTextBlock, text: String): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        retypes += block to text
        val ids = block.runs.map { it.id }
        record { runs = runs.filter { it.id == ids.first() || it.id !in ids }.map { if (it.id == ids.first()) it.copy(text = text) else it } }
        return PdfCoreResult.Success(Unit)
    }

    override fun removeTextBlock(block: ContentTextBlock): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        textDeletes += block
        val ids = block.runs.map { it.id }
        record { runs = runs.filter { it.id !in ids } }
        return PdfCoreResult.Success(Unit)
    }

    override fun moveTextBlock(block: ContentTextBlock, dx: Double, dy: Double): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        textMoves += Triple(block, dx, dy)
        val ids = block.runs.map { it.id }
        record { runs = runs.map { if (it.id in ids) it.copy(bounds = it.bounds.copy(x = it.bounds.x + dx, y = it.bounds.y + dy)) else it } }
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

    // Like the core, a move keeps the size: the box's origin is all that is read.
    override fun movePath(path: ContentPath, to: AnnotationRect): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        pathMoves += path to to
        record { paths = paths.map { if (it.id == path.id) it.copy(bounds = it.bounds.copy(x = to.x, y = to.y)) else it } }
        return PdfCoreResult.Success(Unit)
    }

    override fun resizePath(path: ContentPath, to: AnnotationRect): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        pathResizes += path to to
        record { paths = paths.map { if (it.id == path.id) it.copy(bounds = to) else it } }
        return PdfCoreResult.Success(Unit)
    }

    override fun removePath(path: ContentPath): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        pathDeletes += path
        record { paths = paths.filter { it.id != path.id } }
        return PdfCoreResult.Success(Unit)
    }

    override fun prepareImageReplacement(image: ContentImage): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        prepared += image
        return PdfCoreResult.Success(Unit)
    }

    // The new source fills the same box, so only the undo log shows it.
    override fun replaceImage(image: ContentImage, imageBytes: ByteArray): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        replaces += image to imageBytes
        record { images = images.toList() }
        return PdfCoreResult.Success(Unit)
    }

    override fun insertTextRun(pageIndex: Int, text: String, bounds: AnnotationRect): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        inserts += Triple(pageIndex, text, bounds)
        val run = ContentTextRun(nextId++, pageIndex, bounds, "FIns", ContentFontKind.Standard14, text)
        record { runs = runs + run }
        return PdfCoreResult.Success(Unit)
    }

    override fun stampPlacement(imageBytes: ByteArray, anchor: AnnotationPoint): PdfCoreResult<AnnotationRect> =
        PdfCoreResult.Success(AnnotationRect(anchor.x, anchor.y - 20.0, 40.0, 20.0))

    override fun insertImage(pageIndex: Int, imageBytes: ByteArray, bounds: AnnotationRect): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(it) }
        imageInserts += Triple(pageIndex, imageBytes, bounds)
        val image = ContentImage(nextId++, pageIndex, bounds, "ImIns")
        record { images = images + image }
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
        val before = Content(runs, images, paths)
        edit()
        undoable.addLast(before to Content(runs, images, paths))
        redoable.clear()
    }

    private fun restore(content: Content) {
        runs = content.runs
        images = content.images
        paths = content.paths
    }

    private data class Content(val runs: List<ContentTextRun>, val images: List<ContentImage>, val paths: List<ContentPath>)

    override fun close() = Unit
}
