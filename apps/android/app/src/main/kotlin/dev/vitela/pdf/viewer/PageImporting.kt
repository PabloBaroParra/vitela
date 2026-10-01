package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/** A PDF the user picked to add: its name, for messages, and its bytes. */
class ImportSource(val name: String, val bytes: ByteArray)

/** The import is waiting for [name]'s password; [wrong] once one was already refused. */
data class ImportPasswordPrompt(val name: String, val wrong: Boolean)

/**
 * Add PDFs, from the Organize grid: every page of each picked file goes in
 * after the last page, one file at a time and one undo step per file — the
 * core's `import_pdf` takes one source per call, so an encrypted file's
 * password is asked about that file alone. The core owns every gate, both the
 * open document's and the source's; this owns the queue.
 *
 * A file the core refuses stops the rest, as the Linux shell's import does:
 * the pages already added stay, and the status names what stopped it.
 */
internal class PageImporting(
    private val session: ViewerSession,
    private val annotations: AnnotationEditing,
    private val layout: PageLayout,
) {
    private val state = session.state

    /** Files still to add, head first. Only touched on the main thread. */
    private val queue = ArrayDeque<ImportSource>()
    private var pagesAdded = 0
    private var filesAdded = 0
    private val warnings = mutableListOf<String>()

    fun start(sources: List<ImportSource>) {
        val openDocument = session.document ?: return
        val organize = state.value.organize ?: return
        if (organize.busy || sources.isEmpty()) return
        queue.clear()
        queue.addAll(sources)
        pagesAdded = 0
        filesAdded = 0
        warnings.clear()
        state.value = state.value.copy(organize = organize.copy(busy = true, importWarnings = emptyList()), status = "Adding PDFs...")
        run(openDocument, password = null)
    }

    fun retryWithPassword(password: String) {
        val openDocument = session.document ?: return
        val organize = state.value.organize ?: return
        if (organize.importPassword == null) return
        state.value = state.value.copy(organize = organize.copy(importPassword = null))
        run(openDocument, password)
    }

    /** Gives up on the file whose password was asked, and on every file after it. */
    fun cancelPassword() {
        val prompt = state.value.organize?.importPassword ?: return
        queue.clear()
        finish("Password entry cancelled for ${prompt.name}.")
    }

    fun dismissWarnings() {
        val organize = state.value.organize ?: return
        state.value = state.value.copy(organize = organize.copy(importWarnings = emptyList()))
    }

    /**
     * Adds the queued files until the queue is empty, a file needs a password,
     * or one is refused. One lane hold for the whole run, so no other edit
     * lands between two files, and one layout re-read at the end of it.
     */
    private fun run(openDocument: PdfDocument, password: String?) {
        session.scope.launch {
            session.documentLane.withLock {
                var attempt = password
                var addedThisRun = false
                var stop: String? = null
                var waiting = false
                while (true) {
                    // Replaced while the run waited for the lane: nothing here is about it any more.
                    if (session.document !== openDocument) {
                        queue.clear()
                        return@withLock
                    }
                    val source = queue.firstOrNull() ?: break
                    val result = withContext(session.compute) { openDocument.importPdf(source.bytes, attempt, openDocument.pageCount) }
                    when {
                        result is PdfCoreResult.Success -> {
                            queue.removeFirst()
                            pagesAdded += result.value.pageCount
                            filesAdded++
                            warnings += result.value.warnings.map { "${source.name}: $it" }
                            attempt = null
                            addedThisRun = true
                            layout.markEdited()
                            state.value = state.value.copy(
                                isDirty = true,
                                revision = state.value.revision + 1,
                                importedSourceNames = state.value.importedSourceNames + (result.value.sourceId to source.name),
                            )
                        }
                        result is PdfCoreResult.Failure && result.error.asksForPassword() -> {
                            val organize = state.value.organize
                            if (organize == null) {
                                // The grid that would ask was closed mid-run.
                                stop = "${source.name} needs a password, so it was not added."
                                queue.clear()
                            } else {
                                state.value = state.value.copy(organize = organize.copy(importPassword = ImportPasswordPrompt(source.name, wrong = attempt != null)))
                                waiting = true
                            }
                            break
                        }
                        else -> {
                            stop = "${source.name} could not be added: ${userMessage((result as PdfCoreResult.Failure).error)}"
                            queue.clear()
                            break
                        }
                    }
                }
                if (addedThisRun) {
                    // Every file went in after the last page: no card moved, so every picture stays.
                    val shown = layout.reread(openDocument) { it }
                    annotations.refresh(openDocument)
                    // The re-read left its reason in the status; the summary below would bury it.
                    if (!shown) stop = listOfNotNull(stop, state.value.status).joinToString(" ")
                }
                if (!waiting) finish(stop)
            }
        }
    }

    private fun finish(stop: String?) {
        val organize = state.value.organize
        state.value = state.value.copy(
            organize = organize?.copy(busy = false, importPassword = null, importWarnings = warnings.toList()),
            status = importSummary(pagesAdded, filesAdded, stop),
        )
        pagesAdded = 0
        filesAdded = 0
        warnings.clear()
    }
}

private fun PdfCoreError.asksForPassword(): Boolean = this == PdfCoreError.PasswordRequired || this == PdfCoreError.WrongPassword

/** The status an import leaves: what went in, then what stopped the rest, if anything did. */
internal fun importSummary(pages: Int, files: Int, stop: String?): String {
    if (files == 0) return stop ?: "No PDF was added."
    val added = "Added ${count(pages, "page")} from ${count(files, "PDF")}. Changes are pending save."
    return if (stop == null) added else "$added $stop"
}

private fun count(n: Int, noun: String): String = if (n == 1) "1 $noun" else "$n ${noun}s"
