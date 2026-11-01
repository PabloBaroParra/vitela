package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.ContentClip
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.ContentSelection
import dev.vitela.pdf.core.ContentTextBlock
import dev.vitela.pdf.core.ContentTextRun
import dev.vitela.pdf.core.PageContent
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument

/**
 * A [RetypableDocument] that behaves like the core around a content-edit
 * checkpoint: when [checkpointOnNextEdit] is set, the next edit it accepts leaves
 * the document on a new base, where every item is numbered afresh — [generation]
 * moves on and every id the page reads report is shifted by [SHIFT] for each
 * generation. An item sent with an id from another generation is refused, as the
 * core would refuse a number it no longer holds (it keeps one alias, which a
 * shell must not lean on), so a test fails if the shell did not read again.
 *
 * Only the edits the checkpoint tests make are translated: text blocks, one image
 * move, selections, and pastes. [crossOnNextHistory] makes the next undo or redo
 * step across a checkpoint, which also moves the generation.
 */
internal class CheckpointingDocument(
    private val base: RetypableDocument = RetypableDocument(),
    private val canCheckpoint: Boolean = true,
) : PdfDocument by base {
    var generation = 0L
        private set
    var checkpointOnNextEdit = false
    var crossOnNextHistory = false

    /** The edits refused for naming an item numbered by another base. */
    val staleRefusals = mutableListOf<String>()

    private val shift get() = generation * SHIFT

    override fun contentBaseGeneration(): Long = generation

    override fun contentEditsCanCheckpoint(): Boolean = canCheckpoint

    override fun pageContent(pageIndex: Int): PdfCoreResult<PageContent> = when (val result = base.pageContent(pageIndex)) {
        is PdfCoreResult.Success -> PdfCoreResult.Success(
            PageContent(
                result.value.textRuns.map { it.numbered() },
                result.value.images.map { it.numbered() },
                result.value.paths.map { it.copy(id = it.id + shift) },
                result.value.blocks.map { it.numbered() },
            ),
        )
        is PdfCoreResult.Failure -> result
    }

    override fun retypeTextBlock(block: ContentTextBlock, text: String) = edit { base.retypeTextBlock(block.current() ?: return@edit stale(block.id), text) }

    override fun moveTextBlock(block: ContentTextBlock, dx: Double, dy: Double) = edit { base.moveTextBlock(block.current() ?: return@edit stale(block.id), dx, dy) }

    override fun removeTextBlock(block: ContentTextBlock) = edit { base.removeTextBlock(block.current() ?: return@edit stale(block.id)) }

    override fun moveContentSelection(selection: ContentSelection, dx: Double, dy: Double) =
        edit { base.moveContentSelection(selection.current() ?: return@edit stale(selection.blocks.firstOrNull()?.id ?: selection.images.first().id), dx, dy) }

    override fun removeContentSelection(selection: ContentSelection) =
        edit { base.removeContentSelection(selection.current() ?: return@edit stale(selection.blocks.firstOrNull()?.id ?: selection.images.first().id)) }

    override fun copyContentSelection(selection: ContentSelection): PdfCoreResult<ContentClip> {
        val current = selection.current() ?: return stale(selection.blocks.firstOrNull()?.id ?: selection.images.first().id)
        return base.copyContentSelection(current)
    }

    override fun pasteContent(pageIndex: Int, clip: ContentClip, left: Double, top: Double) = edit { base.pasteContent(pageIndex, clip, left, top) }

    override fun undoAnnotations(): PdfCoreResult<Boolean> = base.undoAnnotations().also { crossIfAsked(it) }

    override fun redoAnnotations(): PdfCoreResult<Boolean> = base.redoAnnotations().also { crossIfAsked(it) }

    private fun crossIfAsked(result: PdfCoreResult<Boolean>) {
        if (crossOnNextHistory && result is PdfCoreResult.Success && result.value) {
            crossOnNextHistory = false
            generation++
        }
    }

    private inline fun edit(apply: () -> PdfCoreResult<Unit>): PdfCoreResult<Unit> {
        val result = apply()
        if (result is PdfCoreResult.Success && checkpointOnNextEdit) {
            checkpointOnNextEdit = false
            generation++
        }
        return result
    }

    private fun <T> stale(id: Long): PdfCoreResult<T> {
        staleRefusals += "item $id is not numbered by generation $generation"
        return PdfCoreResult.Failure(PdfCoreError.Failed("That item is no longer on the page."))
    }

    private fun ContentTextRun.numbered() = copy(id = id + shift)

    private fun ContentImage.numbered() = copy(id = id + shift)

    private fun ContentTextBlock.numbered() = copy(runs = runs.map { it.numbered() })

    private fun Long.isCurrent() = this - shift in 0 until SHIFT

    private fun ContentTextBlock.current(): ContentTextBlock? =
        if (runs.all { it.id.isCurrent() }) copy(runs = runs.map { it.copy(id = it.id - shift) }) else null

    private fun ContentSelection.current(): ContentSelection? {
        val blocks = blocks.map { it.current() ?: return null }
        if (!images.all { it.id.isCurrent() }) return null
        return copy(blocks = blocks, images = images.map { it.copy(id = it.id - shift) })
    }

    companion object {
        /** The ids a generation adds to every item's number. */
        const val SHIFT = 1000L
    }
}
