package dev.vitela.pdf.recovery

/** How often unsaved work is looked for: a crash loses at most about this much of it. */
internal const val AUTOSAVE_INTERVAL_MS = 60_000L

/** Identifies the edit state a recovery copy was taken of. */
internal data class RecoveryRevision(val documentId: Long, val revision: Long)

/** The open document as one autosave attempt sees it. */
internal data class AutosaveObservation(val revision: RecoveryRevision, val unsaved: Boolean, val encrypted: Boolean)

internal enum class AutosaveDecision {
    /** Delete this viewer's entry: nothing unsaved is left to protect, or nothing may be written. */
    Forget,

    /** The entry already holds this revision. */
    Wait,

    /** Serialize the document and replace the entry. */
    Write,
}

/**
 * The rule of each attempt; [written] is the revision the entry on disk holds.
 * Encrypted content is never written: the copy could not be encrypted again,
 * and losing unsaved work is the lesser harm than leaving a decrypted copy of
 * a protected file behind. An earlier copy goes too, since pages of a
 * protected PDF may have been added since.
 */
internal fun autosaveDecision(observed: AutosaveObservation?, written: RecoveryRevision?): AutosaveDecision = when {
    observed == null || !observed.unsaved || observed.encrypted -> AutosaveDecision.Forget
    observed.revision == written -> AutosaveDecision.Wait
    else -> AutosaveDecision.Write
}

/**
 * When autosave tries again after a failure. A full disk fails every attempt
 * the same way, and each one costs a serialization first: so the wait doubles
 * from one interval up to [MAX_DELAY_MS], and the first success resets it.
 * Times are any monotonic clock's milliseconds.
 */
internal class AutosaveBackoff {
    private var failures = 0
    private var retryAt: Long? = null

    fun ready(now: Long): Boolean = retryAt?.let { now >= it } ?: true

    /** Records a failure; true for the first of a streak, the one the user is told about. */
    fun failed(now: Long): Boolean {
        if (failures < Int.MAX_VALUE) failures++
        retryAt = now + delay(failures)
        return failures == 1
    }

    fun succeeded() {
        failures = 0
        retryAt = null
    }

    companion object {
        const val MAX_DELAY_MS = 15 * 60_000L

        /** How long to wait after the [failures]-th failure in a row. */
        fun delay(failures: Int): Long = minOf(AUTOSAVE_INTERVAL_MS shl (failures - 1).coerceIn(0, 16), MAX_DELAY_MS)
    }
}
