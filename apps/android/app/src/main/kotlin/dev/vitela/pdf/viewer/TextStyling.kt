package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.ContentTextRun
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.TextRunLookState
import dev.vitela.pdf.core.TextStyleToggle
import dev.vitela.pdf.text.UiText
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Bold, Italic and Underline on existing text in Edit content, for two targets:
 * the block the retype dialog holds, and the text blocks of a selection. What
 * the toggles show is the first run's look, read from the core with every
 * pending edit in place; a press applies [textStylePress] and sends what it
 * records as one core call — one undo step whatever the run count — then lands
 * through [ContentEditing.landed] like any other edit, which rebuilds the preview
 * and reads the page again.
 *
 * What a restyle writes — a sibling face, or a synthetic one — and whether a
 * really-bold run can be made regular at all is the core's; a style that is the
 * run's own font is refused here, before the core, with its reason.
 *
 * Unlike the Windows shell, which writes as the reader types, the dialog's typed
 * text is not written first: the toggle styles the text as the page now paints
 * it, and the dialog reopens over the restyled block with what was typed kept.
 */
internal class TextStyling(
    private val session: ViewerSession,
    private val content: ContentEditing,
) {
    private val state = session.state

    /**
     * The toggles for [runs], one look per run; null when there are none, the
     * core cannot read one, or refuses. Called with the document lane held.
     */
    suspend fun buttonsFor(document: PdfDocument, runs: List<ContentTextRun>): TextStyleButtons? =
        (looksOf(document, runs) as? Looks.Read)?.states?.let(::textStyleButtons)

    /**
     * Reads the toggles for the selection as it stands, if it holds text, and
     * pairs them with it. Called with the document lane held, after anything
     * that set the selection.
     */
    suspend fun readSelection(document: PdfDocument) {
        val selection = state.value.contentEdit?.selection ?: return
        if (selection.blocks.isEmpty()) return
        val buttons = buttonsFor(document, selection.blocks.flatMap { it.runs }) ?: return
        val mode = state.value.contentEdit ?: return
        if (mode.selection != selection || session.document !== document) return
        state.value = state.value.copy(contentEdit = mode.copy(selectionStyle = SelectionTextStyle(selection, buttons)))
    }

    /**
     * Presses [style] for the open editor's block, for the document [documentId]
     * the dialog was built for. [typed] is what its field holds, kept when the
     * dialog reopens over the restyled block. A refusal is shown in the dialog.
     */
    fun toggleInEditor(documentId: Long, style: TextStyleToggle, typed: String) {
        val openDocument = session.document ?: return
        if (documentId != state.value.documentId) return
        val editor = state.value.contentEdit?.editor ?: return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit?.editor?.block != editor.block) return@withLock
                val refusal = restyle(openDocument, editor.block.runs, style)
                if (refusal != null) {
                    val mode = state.value.contentEdit ?: return@withLock
                    val now = mode.editor?.takeIf { it.block == editor.block } ?: return@withLock
                    state.value = state.value.copy(contentEdit = mode.copy(editor = now.copy(text = typed, error = refusal)))
                    return@withLock
                }
                content.landed(openDocument, editor.block.pageIndex, TEXT_RESTYLED)
                reopen(openDocument, editor, typed)
            }
        }
    }

    /**
     * Presses [style] for every text block of the selection, as one undo step.
     * The selection is kept, looked up again in the fresh read; a refusal is
     * reported in the status line.
     */
    fun toggleSelection(style: TextStyleToggle) {
        val openDocument = session.document ?: return
        val selection = state.value.contentEdit?.selection?.takeIf { it.blocks.isNotEmpty() } ?: return
        session.scope.launch {
            session.documentLane.withLock {
                if (session.document !== openDocument || state.value.contentEdit?.selection != selection) return@withLock
                val refusal = restyle(openDocument, selection.blocks.flatMap { it.runs }, style)
                if (refusal != null) {
                    state.value = state.value.copy(status = refusal)
                    return@withLock
                }
                content.landed(openDocument, selection.pageIndex, TEXT_RESTYLED, settled = { content.selections.follow(selection) })
                readSelection(openDocument)
            }
        }
    }

    /**
     * After a restyle landed: the editor over the block as the page was read
     * again — found by its first run, else by its box — with its toggles read
     * anew and [typed] kept. Closed when the block is gone, and left closed when
     * the core's checkpoint dropped it with everything else the mode held.
     */
    private suspend fun reopen(document: PdfDocument, before: TextBlockEditor, typed: String) {
        val mode = state.value.contentEdit ?: return
        val now = mode.editor?.takeIf { it.block == before.block } ?: return
        val page = mode.blocks[before.block.pageIndex].orEmpty()
        val block = page.firstOrNull { it.id == before.block.id } ?: page.firstOrNull { it.bounds == before.block.bounds }
        if (block == null) {
            state.value = state.value.copy(contentEdit = mode.copy(editor = null), status = TEXT_GONE)
            return
        }
        val reopened = now.copy(block = block, text = typed, error = null, canCheckpoint = content.canCheckpoint(document), style = buttonsFor(document, block.runs))
        val latest = state.value.contentEdit ?: return
        if (latest.editor?.block != before.block) return
        state.value = state.value.copy(contentEdit = latest.copy(editor = reopened))
    }

    /** Records pressing [style] for [runs]; null once the core accepted it, else why nothing was recorded. Called with the lane held. */
    private suspend fun restyle(document: PdfDocument, runs: List<ContentTextRun>, style: TextStyleToggle): UiText? {
        if (!withContext(session.compute) { document.contentEditingAllowed() }) return CONTENT_EDIT_FORBIDDEN
        val states = when (val looks = looksOf(document, runs)) {
            is Looks.Read -> looks.states
            is Looks.Refused -> return looks.reason
        }
        val press = textStylePress(runs.zip(states), style)
        press.refusal?.let { return it }
        // Never empty for a target with runs: the first run always changes.
        if (press.restyles.isEmpty()) return TEXT_GONE
        return when (val result = withContext(session.compute) { document.restyleTextRuns(press.restyles) }) {
            is PdfCoreResult.Success -> null
            is PdfCoreResult.Failure -> userMessage(result.error)
        }
    }

    private sealed interface Looks {
        data class Read(val states: List<TextRunLookState>) : Looks
        data class Refused(val reason: UiText) : Looks
    }

    /** Each run's look, in order, or why one could not be read. */
    private suspend fun looksOf(document: PdfDocument, runs: List<ContentTextRun>): Looks = withContext(session.compute) {
        if (runs.isEmpty()) return@withContext Looks.Refused(TEXT_GONE)
        val states = runs.map { run ->
            when (val result = document.textRunLook(run)) {
                is PdfCoreResult.Success -> result.value ?: return@withContext Looks.Refused(TEXT_STYLE_UNREADABLE)
                is PdfCoreResult.Failure -> return@withContext Looks.Refused(userMessage(result.error))
            }
        }
        Looks.Read(states)
    }
}
