package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.PreparedImport
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/** A PDF the user picked to add: its name, for messages, and its bytes. */
class ImportSource(val name: String, val bytes: ByteArray)

/** The import is waiting for [name]'s password; [wrong] once one was already refused. */
data class ImportPasswordPrompt(val name: String, val wrong: Boolean)

/**
 * Add PDFs, from the Organize grid, in the core's two phases — the Linux and
 * Windows shells' import:
 *
 * 1. every picked file is prepared on its own (`prepare_import`), in pick
 *    order, without touching the document: a locked file asks for its own
 *    password, and a refusal names the file that caused it;
 * 2. once all are prepared, and the reader accepted what their pages would
 *    lose, the whole pick goes in after the last page as ONE undo step
 *    (`import_prepared`).
 *
 * All or nothing: a cancel or a refusal at any point adds no page at all, and
 * every source prepared so far is let go. The core owns every gate, the open
 * document's and each source's; this owns the queue and the questions.
 */
internal class PageImporting(
    private val core: PdfCore?,
    private val session: ViewerSession,
    private val annotations: AnnotationEditing,
    private val layout: PageLayout,
) {
    private val state = session.state

    /** Files still to prepare, head first. Only touched on the main thread. */
    private val queue = ArrayDeque<ImportSource>()

    /** Files prepared so far, in pick order. Only touched on the main thread. */
    private val prepared = mutableListOf<Pair<ImportSource, PreparedImport>>()

    /**
     * Bumped by every start and every [abandon]. Work started for an older
     * pick finds it changed when it comes back from the core and lets go of
     * what it brought, instead of adding it to a pick that no longer exists.
     */
    private var generation = 0

    fun start(sources: List<ImportSource>) {
        val availableCore = core ?: return
        val openDocument = session.document ?: return
        val organize = state.value.organize ?: return
        if (organize.busy || sources.isEmpty()) return
        abandon()
        queue.addAll(sources)
        state.value = state.value.copy(organize = organize.copy(busy = true, importPassword = null, importWarnings = emptyList()), status = "Checking the selected PDFs...")
        prepare(availableCore, openDocument, generation, password = null)
    }

    fun retryWithPassword(password: String) {
        val availableCore = core ?: return
        val openDocument = session.document ?: return
        val organize = state.value.organize ?: return
        if (organize.importPassword == null) return
        state.value = state.value.copy(organize = organize.copy(importPassword = null))
        prepare(availableCore, openDocument, generation, password)
    }

    /** Gives up on the whole pick: nothing was added yet, and nothing will be. */
    fun cancelPassword() {
        val prompt = state.value.organize?.importPassword ?: return
        finish("Password entry cancelled for ${prompt.name}. No PDF was added.")
    }

    /** The reader accepted what the pages will lose: the whole pick goes in. */
    fun acceptWarnings() {
        val openDocument = session.document ?: return
        if (state.value.organize?.importWarnings.isNullOrEmpty()) return
        add(openDocument, generation)
    }

    /** The reader declined what the pages would lose: nothing goes in. */
    fun dismissWarnings() {
        if (state.value.organize?.importWarnings.isNullOrEmpty()) return
        finish("PDF import cancelled.")
    }

    /**
     * Drops the pick in progress, if any, without a word: the grid that asked
     * for it is closing, or the document it was for is going away. Each source
     * prepared so far is closed, which frees its decrypted copy in the core.
     */
    fun abandon() {
        generation++
        queue.clear()
        prepared.forEach { (_, source) -> source.close() }
        prepared.clear()
    }

    /**
     * Prepares the queued files until the queue is empty, a file needs a
     * password, or one is refused. Not on the document lane: preparing reads
     * only the picked bytes, so other edits may land meanwhile — the batch is
     * placed at the page count the lane sees when it is added.
     */
    private fun prepare(availableCore: PdfCore, openDocument: PdfDocument, pick: Int, password: String?) {
        session.scope.launch {
            var attempt = password
            while (true) {
                val source = queue.firstOrNull() ?: break
                val result = withContext(session.compute) { availableCore.prepareImport(source.bytes, attempt) }
                if (!isCurrent(openDocument, pick)) {
                    (result as? PdfCoreResult.Success)?.value?.close()
                    return@launch
                }
                when {
                    result is PdfCoreResult.Success -> {
                        queue.removeFirst()
                        prepared += source to result.value
                        attempt = null
                    }
                    result is PdfCoreResult.Failure && result.error.asksForPassword() -> {
                        val organize = state.value.organize ?: return@launch
                        state.value = state.value.copy(
                            organize = organize.copy(importPassword = ImportPasswordPrompt(source.name, wrong = attempt != null)),
                            status = "Waiting for the password for ${source.name}.",
                        )
                        return@launch
                    }
                    else -> {
                        finish("${source.name} could not be added: ${userMessage((result as PdfCoreResult.Failure).error)} No PDF was added.")
                        return@launch
                    }
                }
            }
            val warnings = prepared.flatMap { (source, ready) -> ready.warnings.map { "${source.name}: $it" } }
            val organize = state.value.organize ?: return@launch
            if (warnings.isEmpty()) add(openDocument, pick)
            else state.value = state.value.copy(organize = organize.copy(importWarnings = warnings), status = "Some content will change on the way in.")
        }
    }

    /**
     * Adds every prepared file at once, after the last page. The sources leave
     * [prepared] before the core sees them, so an [abandon] while the core is
     * busy cannot close one mid-import; they are closed here once it answers.
     */
    private fun add(openDocument: PdfDocument, pick: Int) {
        session.scope.launch {
            session.documentLane.withLock {
                // Replaced or closed while the batch waited for the lane: nothing here is about it any more.
                if (!isCurrent(openDocument, pick)) return@withLock
                val batch = prepared.toList()
                prepared.clear()
                val organize = state.value.organize ?: return@withLock
                state.value = state.value.copy(organize = organize.copy(importWarnings = emptyList()), status = "Adding the selected PDFs...")
                val result = try {
                    withContext(session.compute) { openDocument.importPrepared(batch.map { it.second }, openDocument.pageCount) }
                } finally {
                    batch.forEach { (_, source) -> source.close() }
                }
                when (result) {
                    is PdfCoreResult.Failure -> finish(userMessage(result.error))
                    is PdfCoreResult.Success -> {
                        layout.markEdited()
                        state.value = state.value.copy(
                            isDirty = true,
                            revision = state.value.revision + 1,
                            importedSourceNames = state.value.importedSourceNames + result.value.sourceIds.zip(batch.map { it.first.name }),
                        )
                        // The pick went in after the last page: no card moved, so every picture stays.
                        val shown = layout.reread(openDocument) { it }
                        annotations.refresh(openDocument)
                        val summary = importSummary(result.value.pageCount, batch.size)
                        // The re-read left its reason in the status; the summary must not bury it.
                        finish(if (shown) summary else "$summary ${state.value.status}")
                    }
                }
            }
        }
    }

    private fun isCurrent(openDocument: PdfDocument, pick: Int) =
        pick == generation && session.document === openDocument && state.value.organize != null

    /** Ends the pick with [status], letting go of whatever was prepared. */
    private fun finish(status: String) {
        abandon()
        val organize = state.value.organize
        state.value = state.value.copy(
            organize = organize?.copy(busy = false, importPassword = null, importWarnings = emptyList()),
            status = status,
        )
    }
}

private fun PdfCoreError.asksForPassword(): Boolean = this == PdfCoreError.PasswordRequired || this == PdfCoreError.WrongPassword

/** The status a finished import leaves: what went in. */
internal fun importSummary(pages: Int, files: Int): String =
    "Added ${count(pages, "page")} from ${count(files, "PDF")}. Changes are pending save."

private fun count(n: Int, noun: String): String = if (n == 1) "1 $noun" else "$n ${noun}s"
