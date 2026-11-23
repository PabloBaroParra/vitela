package dev.vitela.pdf.recovery

import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.nio.file.Files
import java.nio.file.NoSuchFileException
import java.nio.file.StandardCopyOption
import java.util.UUID

/** The directory name in the app's no-backup files, which neither a backup nor a device transfer copies. */
internal const val RECOVERY_DIRECTORY_NAME = "recovery"

/**
 * The entries someone in this process owns. The activity can have several
 * instances in one process, each with its own ViewModel; when the process
 * dies this goes with it, which is what makes every entry left an orphan.
 */
class LiveEntries {
    private val keys = HashSet<RecoveryKey>()

    @Synchronized
    fun claim(key: RecoveryKey): Boolean = keys.add(key)

    @Synchronized
    fun release(key: RecoveryKey) {
        keys.remove(key)
    }

    companion object {
        val process = LiveEntries()
    }
}

/**
 * Entries as `<key>.pdf` and `<key>.meta` in [directory]. Each file is written
 * to a sibling, synced and moved over, so it is only ever whole or absent, and
 * an entry missing either one, or whose sidecar does not parse, is discarded.
 */
class FileRecoveryStore(private val directory: File, private val live: LiveEntries = LiveEntries.process) : RecoveryStore {
    override fun newKey(): RecoveryKey = RecoveryKey(UUID.randomUUID().toString()).also(live::claim)

    /**
     * Between the two moves the old sidecar stands beside new bytes or the
     * reverse, so the order keeps the signature flag failing closed: one newly
     * saying a signature broke goes in before the bytes, one clearing it after.
     */
    override fun write(key: RecoveryKey, bytes: ByteArray, meta: RecoveryMeta) {
        directory.mkdirs()
        val sidecar = meta.encode().toByteArray()
        if (meta.breaksSignatures) replace(sidecarOf(key), sidecar)
        replace(pdfOf(key), bytes)
        if (!meta.breaksSignatures) replace(sidecarOf(key), sidecar)
    }

    override fun delete(key: RecoveryKey) {
        // Sidecar first: without it the entry is never offered, even if the rest fails.
        val files = listOf(sidecarOf(key), pdfOf(key))
        (files + files.map(::partialOf)).forEach { it.delete() }
    }

    override fun claimNewest(skip: Set<RecoveryKey>): RecoveryOrphan? = synchronized(live) {
        val keys = directory.list().orEmpty().mapNotNullTo(LinkedHashSet(), ::keyOf) - skip
        val claimed = keys.filter(live::claim)
        val orphans = claimed.mapNotNull(::inspect)
        val newest = orphans.maxByOrNull { it.meta.savedAt }
        claimed.filter { it != newest?.key }.forEach(live::release)
        newest
    }

    override fun read(key: RecoveryKey): ByteArray {
        val bytes = Files.readAllBytes(pdfOf(key).toPath())
        // Anywhere near the start, as readers accept it: an incremental save
        // keeps whatever prefix the original file had.
        if (!String(bytes, 0, minOf(bytes.size, HEADER_REACH), Charsets.ISO_8859_1).contains("%PDF-")) throw CorruptRecoveryException()
        return bytes
    }

    override fun release(key: RecoveryKey) = live.release(key)

    /** The claimed entry if it can be offered. Only a definite verdict deletes: a read that failed may work next time. */
    private fun inspect(key: RecoveryKey): RecoveryOrphan? {
        listOf(sidecarOf(key), pdfOf(key)).forEach { partialOf(it).delete() }
        return try {
            val meta = RecoveryMeta.parse(String(Files.readAllBytes(sidecarOf(key).toPath()), Charsets.UTF_8))
            Files.size(pdfOf(key).toPath())
            if (meta == null) delete(key)
            meta?.let { RecoveryOrphan(key, it) }
        } catch (_: NoSuchFileException) {
            delete(key)
            null
        } catch (_: IOException) {
            null
        }
    }

    private fun pdfOf(key: RecoveryKey) = File(directory, "${key.value}.pdf")
    private fun sidecarOf(key: RecoveryKey) = File(directory, "${key.value}.meta")
    private fun partialOf(file: File) = File(directory, "${file.name}.partial")

    private fun keyOf(name: String): RecoveryKey? = ENTRY_FILE.matchEntire(name)?.let { RecoveryKey(it.groupValues[1]) }

    private fun replace(target: File, bytes: ByteArray) {
        val partial = partialOf(target)
        try {
            FileOutputStream(partial).use {
                it.write(bytes)
                it.fd.sync()
            }
            Files.move(partial.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE)
        } catch (error: Throwable) {
            partial.delete()
            throw error
        }
    }

    private companion object {
        const val HEADER_REACH = 1024
        val ENTRY_FILE = Regex("([0-9a-f-]{36})\\.(?:pdf|meta)(?:\\.partial)?")
    }
}
