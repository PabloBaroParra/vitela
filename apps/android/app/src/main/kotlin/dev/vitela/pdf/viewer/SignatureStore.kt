package dev.vitela.pdf.viewer

import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.nio.file.StandardCopyOption

/** The file name in the app's private files; `data_extraction_rules.xml` excludes it by this name. */
internal const val SIGNATURE_FILE_NAME = "signature.png"

/**
 * The one drawn signature the user asked to be remembered, as the PNG the pad
 * made. Called off the main thread — every method touches storage.
 */
interface SignatureStore {
    /** The remembered PNG, or null when there is none or it cannot be read. */
    fun load(): ByteArray?

    /** Replaces the remembered PNG; false when it could not be written. */
    fun save(png: ByteArray): Boolean

    fun delete()
}

/** Remembers nothing — for a ViewModel built without app storage. */
object NoSignatureStore : SignatureStore {
    override fun load(): ByteArray? = null
    override fun save(png: ByteArray) = false
    override fun delete() = Unit
}

/**
 * The signature kept in [file], inside the app's private files: no other app
 * can read it, it survives updates, and — by the user's choice — it never
 * leaves the phone (backups are off, and device-to-device transfer excludes
 * it). Written to a sibling and moved over, so a crash mid-write leaves the
 * old signature, not half of the new one.
 */
class FileSignatureStore(private val file: File) : SignatureStore {
    override fun load(): ByteArray? = try {
        file.takeIf { it.isFile }?.readBytes()?.takeIf { it.isNotEmpty() }
    } catch (_: IOException) {
        null
    }

    override fun save(png: ByteArray): Boolean {
        val partial = File(file.parentFile, "${file.name}.partial")
        return try {
            file.parentFile?.mkdirs()
            partial.writeBytes(png)
            Files.move(partial.toPath(), file.toPath(), StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE)
            true
        } catch (_: IOException) {
            partial.delete()
            false
        }
    }

    override fun delete() {
        file.delete()
    }
}
