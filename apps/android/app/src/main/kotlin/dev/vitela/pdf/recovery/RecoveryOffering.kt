package dev.vitela.pdf.recovery

import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.viewer.ViewerSession
import dev.vitela.pdf.viewer.ViewerState
import java.io.IOException
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/** Unsaved work a crashed session left behind, as the dialog names it. */
data class RecoveryOffer(val name: String, val savedAt: Long)

internal const val RECOVERY_KEPT = "Your recovered work is kept for the next time Vitela starts."
internal const val RECOVERED = "Recovered unsaved work. Save a copy to keep it."
internal const val RECOVERED_SIGNATURE_BROKEN = "Recovered unsaved work. Saving it would break its signature and is refused. Use Protect to keep it in a new file."
internal const val RECOVERY_DISCARDED = "The recovered copy could not be opened and was discarded."
internal const val RECOVERY_UNREADABLE = "The recovered copy could not be opened. It is kept for the next time Vitela starts."

/** An offer only stands over an idle Home: never over a document, one being opened, or a question about one. */
internal fun recoveryOfferVisible(state: ViewerState): Boolean =
    state.documentId == 0L && !state.isLoading && !state.needsPassword && state.pendingReplacementTitle == null

/**
 * Makes [document] the open one, unsaved, under [name]; [takeOver] hands over
 * the entry it came from at the moment it becomes the open document.
 * Supplied by the ViewModel, which owns opening; the document lane is held.
 */
internal typealias RecoveredInstaller = suspend (name: String, document: PdfDocument, takeOver: () -> RecoveryKey) -> Unit

/**
 * Offers what crashed sessions left behind, one entry at a time: restore it,
 * discard it, or leave everything for a later launch.
 */
internal class RecoveryOffering(
    private val core: PdfCore?,
    private val session: ViewerSession,
    private val store: RecoveryStore,
    private val install: RecoveredInstaller,
) {
    private val state = session.state
    private val _offer = MutableStateFlow<RecoveryOffer?>(null)
    val offer: StateFlow<RecoveryOffer?> = _offer.asStateFlow()

    /** The entry behind [offer], claimed until it is answered. */
    private var orphan: RecoveryOrphan? = null
    /** Entries that failed to open for a reason that may pass: kept, but not asked about again this launch. */
    private val skipped = HashSet<RecoveryKey>()
    private var later = false
    private var looking = false
    private var closed = false

    /** Looks for the next entry to offer. Asked whenever Home is idle. */
    fun look() {
        if (core == null || later || looking || closed || orphan != null || !recoveryOfferVisible(state.value)) return
        looking = true
        // Not cancellable: a claim nobody hears about would never be offered again in this process.
        session.scope.launch(start = CoroutineStart.UNDISPATCHED) {
            withContext(NonCancellable) {
                val found = withContext(session.io) { store.claimNewest(skipped.toSet()) }
                looking = false
                when {
                    found == null -> Unit
                    closed -> store.release(found.key)
                    else -> {
                        orphan = found
                        _offer.value = RecoveryOffer(found.meta.name, found.meta.savedAt)
                    }
                }
            }
        }
    }

    /** Opens the offered entry, unless a document arrived first: then it stays offered for when Home is back. */
    fun restore() {
        val availableCore = core ?: return
        val restoring = orphan ?: return
        session.scope.launch {
            session.documentLane.withLock {
                if (orphan !== restoring || session.document != null || !recoveryOfferVisible(state.value)) return@withLock
                state.value = state.value.copy(isLoading = true, status = "Restoring PDF...")
                val opened = open(availableCore, restoring)
                if (closed) return@withLock
                when (opened) {
                    is Opened.Document -> {
                        install(restoring.meta.name, opened.document) {
                            answered()
                            restoring.key
                        }
                        state.value = state.value.copy(status = if (restoring.meta.breaksSignatures) RECOVERED_SIGNATURE_BROKEN else RECOVERED)
                    }
                    Opened.Discard -> {
                        state.value = state.value.copy(isLoading = false, status = RECOVERY_DISCARDED)
                        discard()
                    }
                    Opened.KeepAndSkip -> {
                        state.value = state.value.copy(isLoading = false, status = RECOVERY_UNREADABLE)
                        answered()
                        skipped += restoring.key
                        store.release(restoring.key)
                        look()
                    }
                }
            }
        }
    }

    /** Deletes the offered entry and moves on to the next one. */
    fun discard() {
        val discarded = orphan ?: return
        if (state.value.isLoading) return
        answered()
        session.scope.launch(start = CoroutineStart.UNDISPATCHED) {
            withContext(NonCancellable) {
                withContext(session.io) { store.delete(discarded.key) }
                store.release(discarded.key)
            }
        }
        look()
    }

    /** Keeps every entry and asks no more this launch. */
    fun keepForLater() {
        val kept = orphan ?: return
        if (state.value.isLoading) return
        later = true
        answered()
        store.release(kept.key)
        state.value = state.value.copy(status = RECOVERY_KEPT)
    }

    fun close() {
        closed = true
        orphan?.let { store.release(it.key) }
        answered()
    }

    private fun answered() {
        orphan = null
        _offer.value = null
    }

    private suspend fun open(core: PdfCore, restoring: RecoveryOrphan): Opened {
        // Caught where it is thrown: an entry this process cannot hold may fit in the next one.
        var failure: Opened = Opened.KeepAndSkip
        val bytes = withContext(session.io) {
            try {
                store.read(restoring.key)
            } catch (_: CorruptRecoveryException) {
                failure = Opened.Discard
                null
            } catch (_: IOException) {
                null
            } catch (_: OutOfMemoryError) {
                null
            }
        } ?: return failure
        // Not cancellable: a handle nobody hears about would never be closed.
        return withContext(NonCancellable) {
            val result = withContext(session.compute) { core.openFromBytes(bytes, null) }
            if (closed) (result as? PdfCoreResult.Success)?.value?.close()
            when (result) {
                is PdfCoreResult.Success -> Opened.Document(if (restoring.meta.breaksSignatures) SignatureBrokenDocument(result.value) else result.value)
                is PdfCoreResult.Failure -> when (result.error) {
                    // Autosave never writes an encrypted copy, so these bytes are not one it wrote.
                    PdfCoreError.PasswordRequired, PdfCoreError.WrongPassword -> Opened.Discard
                    // A parser refusal and a passing fault come back alike: never delete the only copy on a guess.
                    is PdfCoreError.Failed -> Opened.KeepAndSkip
                }
            }
        }
    }

    private sealed interface Opened {
        class Document(val document: PdfDocument) : Opened
        data object Discard : Opened
        data object KeepAndSkip : Opened
    }
}
