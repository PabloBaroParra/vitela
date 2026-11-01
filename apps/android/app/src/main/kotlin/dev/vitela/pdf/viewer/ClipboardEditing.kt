package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.ContentClip
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.ContentPath
import dev.vitela.pdf.core.ContentSelection
import dev.vitela.pdf.core.ContentTextBlock
import dev.vitela.pdf.core.PageContent
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

// Wording follows the Windows and Linux shells'; a touch screen has no Ctrl+V, so the hints name the tile and a tap instead.
internal const val CONTENT_PASTE_PROMPT = "Tap the page where the pasted content's top-left corner should go."
internal const val CONTENT_PASTE_CANCELLED = "Paste cancelled."
internal const val CONTENT_PASTED = "Pasted. Save to keep the change."

/** What a copy or a cut tells the reader to do next. */
internal const val CONTENT_COPIED_HINT = " Open Paste content, then tap where it should go."

/** What the status line says once [count] items were copied or cut — [verb] is "Copied" or "Cut". */
internal fun copiedMessage(verb: String, count: Int) = "$verb $count ${if (count == 1) "item" else "items"}."

/** What the status line says once a paste of [count] items landed and they are selected. */
internal fun pastedMessage(count: Int) = "Pasted $count ${if (count == 1) "item" else "items"} — drag inside the dashed box to place them."

/** The ids a page's content had when it was last read, to tell what a paste added. */
internal data class PageIds(val runs: Set<Long>, val images: Set<Long>, val paths: Set<Long>)

/** [content]'s ids: every run of every block, since a block's own id is only its first run's. */
internal fun pageIdsOf(content: PageContent) = PageIds(
    runs = content.blocks.flatMap { block -> block.runs.map { it.id } }.toSet(),
    images = content.images.map { it.id }.toSet(),
    paths = content.paths.map { it.id }.toSet(),
)

/**
 * What a paste added to page [pageIndex]: the items of the fresh read that
 * [before] did not have, or null when it added none. A block counts when any of
 * its runs is new, so one that grouped with a run already there is still found
 * — and then holds that run too, which the core refuses to move without the rest.
 * The core does not say which items are the paste's, so this is a difference of
 * ids, not a lookup.
 */
internal fun selectionOfPaste(
    pageIndex: Int,
    before: PageIds,
    blocks: List<ContentTextBlock>,
    images: List<ContentImage>,
    paths: List<ContentPath>,
): ContentSelection? =
    ContentSelection(
        pageIndex,
        blocks.filter { block -> block.runs.any { it.id !in before.runs } },
        images.filter { it.id !in before.images },
        paths.filter { it.id !in before.paths },
    ).takeIf { it.count > 0 }

/**
 * The clipboard half of Edit content (B25 T-209): Copy and Cut on a selection
 * fill a clip, and Paste content arms a page tap that puts it down. The clip is
 * a self-contained one-page PDF the core hands back; it is held by the
 * [session], not by the mode or the document, so it survives a document being
 * replaced and pastes into the next one. The core keeps no clipboard of its own.
 *
 * A paste is one undo step and its items are selected afterwards, so a drag
 * moves all of them as that same step (the core folds it into the paste). The
 * core refuses to edit part of a fresh paste or to delete it before a save;
 * those refusals are shown as the core words them.
 *
 * [content] owns the mode and lands each accepted edit.
 */
internal class ClipboardEditing(
    private val session: ViewerSession,
    private val content: ContentEditing,
) {
    private val state = session.state

    /** Copies the selection into the clip, changing nothing in the document and keeping the selection. */
    fun copy() {
        val openDocument = session.document ?: return
        val selection = state.value.contentEdit?.selection ?: return
        session.scope.launch {
            session.documentLane.withLock {
                // Only the selection this tap was made on: one cut or ended meanwhile copies nothing.
                if (session.document !== openDocument || state.value.contentEdit?.selection != selection) return@withLock
                when (val result = withContext(session.compute) { openDocument.copyContentSelection(selection) }) {
                    is PdfCoreResult.Success -> hold(result.value, copiedMessage("Copied", selection.count) + CONTENT_COPIED_HINT)
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }

    /**
     * Copies the selection into the clip and deletes it from the page, as one
     * undo step. Spent before the core answers, like a delete: a second tap on
     * Cut finds nothing selected. A refusal is shown and the selection given back.
     */
    fun cut() {
        val openDocument = session.document ?: return
        val mode = state.value.contentEdit ?: return
        val selection = mode.selection ?: return
        state.value = state.value.copy(contentEdit = mode.copy(selection = null))
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit == null) return@withLock
                when (val result = withContext(session.compute) { openDocument.cutContentSelection(selection) }) {
                    is PdfCoreResult.Success -> {
                        session.contentClip = result.value
                        state.value.contentEdit?.let { state.value = state.value.copy(contentEdit = it.copy(hasClip = true)) }
                        // The core no longer reports the members, so their outlines go too.
                        content.landed(openDocument, selection.pageIndex, copiedMessage("Cut", selection.count) + CONTENT_COPIED_HINT)
                    }
                    is PdfCoreResult.Failure -> {
                        val now = state.value.contentEdit ?: return@withLock
                        val back = if (now.selection == null && now.idle) selection else now.selection
                        state.value = state.value.copy(contentEdit = now.copy(selection = back), status = userMessage(result.error))
                    }
                }
            }
        }
    }

    /** Arms the next page tap to put the held clip down; arms nothing while none is held. */
    fun armPaste() {
        val mode = state.value.contentEdit ?: return
        if (session.contentClip == null) return
        state.value = state.value.copy(
            contentEdit = mode.copy(adding = ContentAddition.Paste, moving = null, editor = null, resizer = null, shaper = null, inserter = null, selection = null),
            status = CONTENT_PASTE_PROMPT,
        )
    }

    /**
     * The armed tap at [point] on page [pageIndex]: pastes the held clip with
     * the top-left corner of its box there, then selects what it added. Spent at
     * once, like an armed insert, so a second tap before the core answers opens
     * what it hits.
     */
    fun place(openDocument: PdfDocument, pageIndex: Int, point: AnnotationPoint) {
        val mode = state.value.contentEdit ?: return
        val clip = session.contentClip ?: return
        state.value = state.value.copy(contentEdit = mode.copy(adding = null))
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit == null) return@withLock
                // What the page held before, so the paste's own items can be told from it.
                val before = content.contentOf(openDocument, pageIndex)?.let(::pageIdsOf) ?: return@withLock
                val readAt = content.readGeneration
                when (val result = withContext(session.compute) { openDocument.pasteContent(pageIndex, clip, point.x, point.y) }) {
                    is PdfCoreResult.Success -> {
                        var pasted: ContentSelection? = null
                        content.landed(openDocument, pageIndex, CONTENT_PASTED, settled = { pasted = selectPasted(pageIndex, before, readAt) })
                        pasted?.let { state.value = state.value.copy(status = pastedMessage(it.count)) }
                    }
                    is PdfCoreResult.Failure -> state.value = state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }

    /** Keeps [clip] for the next paste and says [status]. */
    private fun hold(clip: ContentClip, status: String) {
        session.contentClip = clip
        val mode = state.value.contentEdit ?: return
        state.value = state.value.copy(contentEdit = mode.copy(hasClip = true), status = status)
    }

    /**
     * Selects what the paste added to page [pageIndex], from the read just made; returns it.
     * Nothing is selected when the paste made the core checkpoint — the generation the pages
     * are read at is no longer [readAt]: [before] holds ids of a parse that is gone, so a
     * difference against it would pick the wrong items. The paste itself stands; the reader
     * taps what they want.
     */
    private fun selectPasted(pageIndex: Int, before: PageIds, readAt: Long?): ContentSelection? {
        if (content.readGeneration != readAt) return null
        val mode = state.value.contentEdit ?: return null
        val pasted = selectionOfPaste(pageIndex, before, mode.blocks[pageIndex].orEmpty(), mode.images[pageIndex].orEmpty(), mode.paths[pageIndex].orEmpty())
        state.value = state.value.copy(contentEdit = mode.copy(selection = pasted))
        return pasted
    }
}
