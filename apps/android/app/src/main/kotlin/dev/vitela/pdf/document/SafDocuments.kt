package dev.vitela.pdf.document

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract

/** A picked PDF: its bytes, and where **Save** may write it back to (null when it may not). */
class OpenedDocument(val displayName: String, val bytes: ByteArray, val saveTarget: String?)

/** A document made with `CreateDocument` that the app will reopen: its name, and its save target. */
class CreatedDocument(val displayName: String, val saveTarget: String?)

/**
 * Storage Access Framework I/O for the shell. The ViewModel only ever sees
 * bytes and an opaque save-target string; everything that knows a SAF [Uri]
 * lives here.
 */
object SafDocuments {
    /** Reads a document picked with `OpenDocument`, or null if it could not be read. */
    fun open(resolver: ContentResolver, uri: Uri): OpenedDocument? {
        val read = read(resolver, uri) ?: return null
        return OpenedDocument(read.displayName, read.bytes, persistAccess(resolver, uri))
    }

    /**
     * Reads a document picked only to be read once — a PDF whose pages are
     * being added — without keeping access to it: nothing will write back to
     * it, and nothing reopens it.
     */
    fun read(resolver: ContentResolver, uri: Uri): OpenedDocument? {
        val bytes = runCatching { resolver.openInputStream(uri)?.use { it.readBytes() } }.getOrNull() ?: return null
        return OpenedDocument(displayName(uri), bytes, saveTarget = null)
    }

    private fun displayName(uri: Uri): String = uri.lastPathSegment?.substringAfterLast('/') ?: "Document.pdf"

    /**
     * Describes a URI picked with `CreateDocument` as a document the app is
     * about to reopen, keeping access so **Save** can write back to it.
     */
    fun created(resolver: ContentResolver, uri: Uri): CreatedDocument =
        CreatedDocument(displayName(uri), persistAccess(resolver, uri))

    /** Writes [bytes] to a URI the user picked with `CreateDocument`. */
    fun writeCopy(resolver: ContentResolver, uri: Uri, bytes: ByteArray): Boolean = write(resolver, uri, bytes)

    /** Writes [bytes] back over the document a save target came from. */
    fun writeBack(resolver: ContentResolver, saveTarget: String, bytes: ByteArray): Boolean = write(resolver, Uri.parse(saveTarget), bytes)

    /**
     * Keeps access to [uri] across process death and returns it as a save
     * target when the provider will accept a write. Write access is only
     * requested when the provider advertises it: asking for a flag the picker
     * never granted throws, and would cost us the read grant too.
     */
    private fun persistAccess(resolver: ContentResolver, uri: Uri): String? {
        val writable = supportsWrite(resolver, uri)
        val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or if (writable) Intent.FLAG_GRANT_WRITE_URI_PERMISSION else 0
        val persisted = runCatching { resolver.takePersistableUriPermission(uri, flags) }.isSuccess
        if (!persisted) runCatching { resolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION) }
        return uri.toString().takeIf { writable && persisted }
    }

    private fun supportsWrite(resolver: ContentResolver, uri: Uri): Boolean = runCatching {
        resolver.query(uri, arrayOf(DocumentsContract.Document.COLUMN_FLAGS), null, null, null)?.use { cursor ->
            cursor.moveToFirst() && cursor.getInt(0) and DocumentsContract.Document.FLAG_SUPPORTS_WRITE != 0
        } ?: false
    }.getOrDefault(false)

    /**
     * Mode "wt", not "w": several providers open "w" without truncating, so a
     * save shorter than the original would leave the old file's tail behind
     * the new trailer — a corrupt PDF that still opens in some readers.
     */
    private fun write(resolver: ContentResolver, uri: Uri, bytes: ByteArray): Boolean = runCatching {
        resolver.openOutputStream(uri, "wt")?.use { it.write(bytes) } ?: error("No output stream")
    }.isSuccess
}
