package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.sync.Mutex

/**
 * What every feature of the viewer shares: the published state, the open
 * document, and the lane that serializes edits to it. Owned by
 * [ViewerViewModel]; each feature holds the same instance, so there is one
 * state and one document no matter which feature last touched them.
 *
 * Core calls run on [compute] and shell file writes on [io], never on a
 * dispatcher named inline: a test hands in its own scheduler so the work
 * stays on the test thread instead of racing it from a real worker.
 */
internal class ViewerSession(
    private val scopeProvider: () -> CoroutineScope,
    initial: ViewerState,
    val compute: CoroutineDispatcher,
    val io: CoroutineDispatcher,
) {
    /**
     * Resolved on each launch, never at construction: reading `viewModelScope`
     * initializes `Dispatchers.Main`, and a ViewModel built without a core
     * (or in a JVM test before `setMain`) must not do that just by existing.
     */
    val scope: CoroutineScope get() = scopeProvider()

    val state = MutableStateFlow(initial)
    var document: PdfDocument? = null

    /** Serializes mutation, save, and replacement snapshots for this ViewModel's lifecycle. */
    val documentLane = Mutex()
}

internal fun userMessage(error: PdfCoreError): String = when (error) {
    PdfCoreError.PasswordRequired, PdfCoreError.WrongPassword -> "This document requires a password."
    is PdfCoreError.Failed -> error.message
}

/** The open file's name without its `.pdf`, for a derived file's suggested name; "Document" when there is none. */
internal fun documentStem(title: String): String =
    title.trim().let { if (it.endsWith(".pdf", ignoreCase = true)) it.dropLast(4) else it }.ifBlank { "Document" }
