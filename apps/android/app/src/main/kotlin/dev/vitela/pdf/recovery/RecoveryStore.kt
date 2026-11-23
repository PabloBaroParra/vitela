package dev.vitela.pdf.recovery

import java.io.IOException
import java.net.URLDecoder
import java.net.URLEncoder
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Locale

/** Names one viewer's entry. */
@JvmInline
value class RecoveryKey(val value: String)

/**
 * The sidecar beside an entry's bytes: when they were written ([savedAt], in
 * epoch milliseconds), the document's display name, and whether the bytes
 * already broke a signature the original file carries — which, once they are
 * reopened, the core can no longer tell.
 *
 * `field TAB value` lines under a versioned header, as on the desktop. An
 * unknown field is skipped, so a later version can add one.
 */
data class RecoveryMeta(val savedAt: Long, val name: String, val breaksSignatures: Boolean) {
    fun encode(): String =
        "$HEADER\nsaved\t$savedAt\nname\t${URLEncoder.encode(name, "UTF-8")}\nbreaks-signatures\t${if (breaksSignatures) 1 else 0}\n"

    companion object {
        private const val HEADER = "vitela-recovery 1"

        /** Null for another format, or without the time or the name an entry is offered by. */
        fun parse(text: String): RecoveryMeta? {
            val lines = text.lines()
            if (lines.first() != HEADER) return null
            val fields = lines.drop(1).mapNotNull { line ->
                val tab = line.indexOf('\t')
                if (tab < 0) null else line.substring(0, tab) to line.substring(tab + 1)
            }.toMap()
            val savedAt = fields["saved"]?.toLongOrNull() ?: return null
            val name = fields["name"]?.let { runCatching { URLDecoder.decode(it, "UTF-8") }.getOrNull() } ?: return null
            // Fails closed: only an explicit "0" clears it.
            return RecoveryMeta(savedAt, name, breaksSignatures = fields["breaks-signatures"] != "0")
        }
    }
}

/** When the copy was written, as the offer says it. */
internal fun savedAtText(savedAt: Long, zone: ZoneId, locale: Locale = Locale.getDefault()): String =
    Instant.ofEpochMilli(savedAt).atZone(zone).format(DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT).withLocale(locale))

/** An entry nobody alive owns, claimed by whoever asked for it. */
class RecoveryOrphan(val key: RecoveryKey, val meta: RecoveryMeta)

/** The entry's bytes can never be restored; any other read failure says nothing about them. */
class CorruptRecoveryException : IOException("The recovered file is not a PDF.")

/**
 * The recovery entries on disk. Everything but [newKey] and [release] touches
 * storage, so it is called off the main thread. A key is *live* from
 * [newKey] or [claimNewest] until [release]: no one else is offered it.
 */
interface RecoveryStore {
    fun newKey(): RecoveryKey

    /** Replaces the entry. */
    @Throws(IOException::class)
    fun write(key: RecoveryKey, bytes: ByteArray, meta: RecoveryMeta)

    /** Deletes the entry's files; the key stays live. */
    fun delete(key: RecoveryKey)

    /** Claims the newest entry that is neither live nor in [skip], removing the remains of writes cut short. */
    fun claimNewest(skip: Set<RecoveryKey>): RecoveryOrphan?

    /** The entry's bytes; [CorruptRecoveryException] when they are not a PDF. */
    @Throws(IOException::class)
    fun read(key: RecoveryKey): ByteArray

    /** Lets go of a live key; its files, if any, are an orphan from now on. */
    fun release(key: RecoveryKey)
}

/** Keeps nothing — for a ViewModel built without app storage. */
object NoRecoveryStore : RecoveryStore {
    override fun newKey() = RecoveryKey("none")
    override fun write(key: RecoveryKey, bytes: ByteArray, meta: RecoveryMeta) = Unit
    override fun delete(key: RecoveryKey) = Unit
    override fun claimNewest(skip: Set<RecoveryKey>): RecoveryOrphan? = null
    override fun read(key: RecoveryKey): ByteArray = throw IOException("No recovery storage.")
    override fun release(key: RecoveryKey) = Unit
}
