package dev.vitela.pdf.recovery

import dev.vitela.pdf.R
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RecoverySnapshot
import dev.vitela.pdf.text.UiText
import dev.vitela.pdf.viewer.ViewerSession
import java.io.IOException
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

internal val AUTOSAVE_UNAVAILABLE: UiText = UiText.Resource(R.string.status_recovery_autosave_unavailable)

/**
 * Keeps a recovery copy of the open document's unsaved work in this viewer's
 * own entry, and deletes it once the work is saved, discarded or replaced.
 * Only bytes are kept, never the undo history.
 *
 * Every field belongs to the main thread; only the core and the disk are
 * elsewhere. The copy is serialized under the document lane, then written
 * under [diskLane], which also orders a deletion after a write it overlaps:
 * [epoch] tells a write that has not started that its work was forgotten, and
 * one that has is deleted right behind. Both outlive the viewer being cleared,
 * so the entry stays live until the last of them is done, and the document's
 * handle stays open until the last snapshot already asked for is.
 *
 * [clock] stamps the copy; [elapsed] is a monotonic one, for the backoff.
 */
internal class Autosaving(
    private val session: ViewerSession,
    private val store: RecoveryStore,
    private val clock: () -> Long,
    private val elapsed: () -> Long,
) {
    private val snapshotLane = Mutex()
    private val diskLane = Mutex()
    private var key: RecoveryKey? = null
    /** Whether [key]'s entry may have files to delete. */
    private var onDisk = false
    private var written: RecoveryRevision? = null
    /** The open document's name. Not the state's title, which a failed open leaves at the other file's. */
    private var documentName = ""
    private var backoff = AutosaveBackoff()
    private var epoch = 0L
    private var started = false
    private var closed = false
    private var running = 0
    /** The document [documentName] and the state describe; the session's can be ahead of them mid-replacement. */
    private var installed: PdfDocument? = null
    /** A cleared viewer's document, until no snapshot needs it. */
    private var leaving: PdfDocument? = null
    private var snapshots = 0

    /** Starts the timer. Production only: a timer in a test scheduler never runs out. */
    fun start() {
        if (started) return
        started = true
        session.scope.launch {
            while (true) {
                delay(AUTOSAVE_INTERVAL_MS)
                outliving { snapshot(lastChance = false) }
            }
        }
    }

    /**
     * The app is leaving the screen, the moment after which the system may
     * kill it. Tried even while backing off: there may not be a next time.
     */
    fun snapshotNow() {
        if (!closed) launchOutliving { snapshot(lastChance = true) }
    }

    /**
     * [name] was installed as the open document, so the one it replaced is
     * gone for good. A restored one keeps protecting itself in [recovered],
     * the entry it came from, which already holds exactly this revision.
     */
    fun documentInstalled(name: String, recovered: RecoveryKey?) {
        forget()
        documentName = name
        installed = session.document
        if (recovered == null) return
        key?.let { previous -> launchOutliving { diskLane.withLock { store.release(previous) } } }
        key = recovered
        onDisk = true
        written = session.state.value.let { RecoveryRevision(it.documentId, it.revision) }
    }

    /** The work was saved, discarded or replaced: its entry goes, and so does any failure streak. */
    fun forget() {
        epoch++
        written = null
        backoff = AutosaveBackoff()
        val forgotten = key ?: return
        if (!onDisk) return
        onDisk = false
        launchOutliving { diskLane.withLock { withContext(session.io) { store.delete(forgotten) } } }
    }

    /**
     * The viewer is going away without a decision about its work: the entry
     * stays, for the next one to offer. Closes [document], at once unless a
     * snapshot asked for before — the app just left the screen — still needs it.
     */
    fun close(document: PdfDocument?) {
        closed = true
        leaving = document
        closeWhenDone()
        releaseWhenDone()
    }

    private suspend fun snapshot(lastChance: Boolean) {
        snapshots++
        try {
            snapshotLane.withLock {
                val taken = session.documentLane.withLock { take(lastChance) } ?: return@withLock
                diskLane.withLock { write(taken) }
            }
        } finally {
            snapshots--
            closeWhenDone()
        }
    }

    private suspend fun take(lastChance: Boolean): Taken? {
        val document = session.document ?: leaving
        // Cleared in the middle of a close or a replacement: there is nothing the state describes to write.
        if (closed && (document == null || document !== installed)) return null
        val state = session.state.value
        val revision = RecoveryRevision(state.documentId, state.revision)
        val observed = document?.let {
            val encrypted = state.isDirty && withContext(session.compute) { runCatching { it.holdsEncryptedContent() }.getOrDefault(true) }
            AutosaveObservation(revision, state.isDirty, encrypted)
        }
        when (autosaveDecision(observed, written)) {
            AutosaveDecision.Forget -> forget()
            AutosaveDecision.Wait -> Unit
            AutosaveDecision.Write -> if (document != null && (lastChance || backoff.ready(elapsed()))) {
                val startedAt = epoch
                when (val result = withContext(session.compute) { document.recoverySnapshot() }) {
                    is PdfCoreResult.Success -> return Taken(result.value, revision, documentName, startedAt)
                    is PdfCoreResult.Failure -> failed()
                }
            }
        }
        return null
    }

    private suspend fun write(taken: Taken) {
        if (epoch != taken.epoch) return
        val entry = key ?: store.newKey().also { key = it }
        onDisk = true
        val meta = RecoveryMeta(clock(), taken.name, taken.snapshot.breaksSignatures)
        val landed = withContext(session.io) {
            try {
                store.write(entry, taken.snapshot.bytes, meta)
                true
            } catch (_: IOException) {
                false
            } catch (_: OutOfMemoryError) {
                false
            }
        }
        // Forgotten while writing: the deletion is already waiting for this lane.
        if (epoch != taken.epoch) return
        if (!landed) return failed()
        written = taken.revision
        backoff.succeeded()
    }

    /** Said once per streak: the work is still on screen, and only a crash would cost it. */
    private fun failed() {
        if (closed) return
        if (backoff.failed(elapsed())) session.state.value = session.state.value.copy(status = AUTOSAVE_UNAVAILABLE)
    }

    /** Undispatched, so it starts — and counts as running — even on a scope about to be cancelled. */
    private fun launchOutliving(block: suspend () -> Unit) {
        session.scope.launch(start = CoroutineStart.UNDISPATCHED) { outliving(block) }
    }

    private suspend fun outliving(block: suspend () -> Unit) {
        running++
        try {
            withContext(NonCancellable) { block() }
        } finally {
            running--
            releaseWhenDone()
        }
    }

    private fun closeWhenDone() {
        if (!closed || snapshots > 0) return
        val document = leaving
        leaving = null
        document?.close()
    }

    private fun releaseWhenDone() {
        if (!closed || running > 0) return
        key?.let(store::release)
        key = null
    }

    private class Taken(val snapshot: RecoverySnapshot, val revision: RecoveryRevision, val name: String, val epoch: Long)
}
