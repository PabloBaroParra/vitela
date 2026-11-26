package dev.vitela.pdf.document

import java.io.File
import java.io.IOException
import java.util.UUID

/**
 * The private copies **Share** hands to other apps: one fresh folder per
 * share under the cache's `share/` root, holding the document under its own
 * name. Plain [File]s only — the `Uri` grant and the share sheet live in
 * [ShareSheet].
 *
 * A copy is not deleted when the chooser closes: the receiving app may still
 * be reading it. Each later share sweeps copies older than [LIFETIME_MILLIS],
 * as the Windows shell does with its temp share folder.
 */
object ShareCopies {
    /** How long a shared copy outlives its share. */
    const val LIFETIME_MILLIS: Long = 60L * 60L * 1000L

    /** The name the cache folder takes under `cacheDir`; `res/xml/share_paths.xml` grants only this. */
    const val ROOT_NAME = "share"

    private const val FALLBACK_NAME = "Document"
    private const val EXTENSION = ".pdf"

    /** Most Android file systems cap a name at 255 bytes. */
    private const val MAX_NAME_BYTES = 255

    /** Characters no file name may hold on at least one file system a receiver might save to. */
    private const val FORBIDDEN = "\\/:*?\"<>|"

    /**
     * [displayName] as a file name: forbidden and control characters become
     * `_`, so it can never leave its folder; a blank name becomes "Document";
     * the `.pdf` extension is added when missing, kept as is when present.
     */
    fun fileName(displayName: String): String {
        val cleaned = displayName.trim()
            .map { if (it in FORBIDDEN || it.isISOControl()) '_' else it }
            .joinToString("")
            .trim()
            .ifEmpty { FALLBACK_NAME }
        val hasExtension = cleaned.endsWith(EXTENSION, ignoreCase = true)
        val stem = if (hasExtension) cleaned.dropLast(EXTENSION.length) else cleaned
        val extension = if (hasExtension) cleaned.takeLast(EXTENSION.length) else EXTENSION
        return fitted(stem, MAX_NAME_BYTES - extension.length).ifEmpty { FALLBACK_NAME } + extension
    }

    /**
     * Writes [bytes] as [displayName] into a new folder under [root], after
     * sweeping copies older than the lifetime. Null when any step fails; a
     * folder this call started is removed again.
     */
    fun write(
        root: File,
        displayName: String,
        bytes: ByteArray,
        now: Long,
        newFolderId: () -> String = { UUID.randomUUID().toString() },
    ): File? {
        sweep(root, now)
        val folder = File(root, newFolderId())
        return try {
            if (!folder.mkdirs()) return null
            File(folder, fileName(displayName)).also { it.writeBytes(bytes) }
        } catch (error: IOException) {
            folder.deleteRecursively()
            null
        } catch (error: SecurityException) {
            folder.deleteRecursively()
            null
        }
    }

    /**
     * Deletes the share folders under [root] last modified more than the
     * lifetime before [now]. Loose files are not ours and stay. Never throws:
     * a sweep that cannot run must not block the share; the next one retries.
     */
    fun sweep(root: File, now: Long) {
        val entries = try {
            root.listFiles().orEmpty()
        } catch (error: SecurityException) {
            return
        }
        entries.forEach { entry ->
            try {
                if (entry.isDirectory && now - entry.lastModified() > LIFETIME_MILLIS) entry.deleteRecursively()
            } catch (error: SecurityException) {
                // Left for the next share.
            }
        }
    }

    /** The longest prefix of [text] that fits in [maxBytes] of UTF-8, never splitting a surrogate pair. */
    private fun fitted(text: String, maxBytes: Int): String {
        if (text.toByteArray(Charsets.UTF_8).size <= maxBytes) return text
        val out = StringBuilder()
        var used = 0
        var index = 0
        while (index < text.length) {
            val end = if (Character.isHighSurrogate(text[index]) && index + 1 < text.length) index + 2 else index + 1
            val piece = text.substring(index, end)
            val size = piece.toByteArray(Charsets.UTF_8).size
            if (used + size > maxBytes) break
            out.append(piece)
            used += size
            index = end
        }
        return out.toString().trim()
    }
}
