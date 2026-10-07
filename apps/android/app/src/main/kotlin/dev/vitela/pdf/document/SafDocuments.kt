package dev.vitela.pdf.document

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns

/**
 * A picked PDF: its bytes, where **Save** may write it back to (null when it
 * may not), and the key Home's Recent list may reopen it by — null when the
 * grant was not persistable, as with "Open with", a share or a drop.
 */
class OpenedDocument(val displayName: String, val bytes: ByteArray, val saveTarget: String?, val recent: String? = null)

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
        val access = persistAccess(resolver, uri)
        return OpenedDocument(read.displayName, read.bytes, access.saveTarget, uri.toString().takeIf { access.readable })
    }

    /**
     * Gives back every grant the app holds on a Recent entry's document. The
     * list is how the user sees what the app can still reach; a removed card
     * must not leave that access behind.
     */
    fun forget(resolver: ContentResolver, recent: String) {
        val uri = Uri.parse(recent)
        resolver.persistedUriPermissions.filter { it.uri == uri }.forEach { permission ->
            val flags = (if (permission.isReadPermission) Intent.FLAG_GRANT_READ_URI_PERMISSION else 0) or
                (if (permission.isWritePermission) Intent.FLAG_GRANT_WRITE_URI_PERMISSION else 0)
            runCatching { resolver.releasePersistableUriPermission(uri, flags) }
        }
    }

    /**
     * Whether a Recent entry's document can still be reached: the grant is
     * still held and its provider still knows the document. A provider
     * answers a deleted document with no row (DocumentsProvider turns its
     * FileNotFoundException into a null cursor); any other failure — an
     * unreachable cloud provider, say — counts as still there, so a card is
     * never dropped for being offline.
     */
    fun isAvailable(resolver: ContentResolver, recent: String): Boolean {
        val uri = Uri.parse(recent)
        if (resolver.persistedUriPermissions.none { it.uri == uri && it.isReadPermission }) return false
        return try {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { it.moveToFirst() } ?: false
        } catch (_: SecurityException) {
            false
        } catch (_: RuntimeException) {
            true
        }
    }

    /**
     * Reads a document picked only to be read once — a PDF whose pages are
     * being added — without keeping access to it: nothing will write back to
     * it, and nothing reopens it.
     */
    fun read(resolver: ContentResolver, uri: Uri): OpenedDocument? {
        val bytes = runCatching { resolver.openInputStream(uri)?.use { it.readBytes() } }.getOrNull() ?: return null
        return OpenedDocument(displayName(resolver, uri), bytes, saveTarget = null)
    }

    /**
     * The provider's own name for the document, falling back to the URI's last
     * segment. Documents handed over by another app (a mail attachment, say)
     * often end their URI in an opaque id rather than a file name.
     */
    private fun displayName(resolver: ContentResolver, uri: Uri): String {
        val provided = runCatching {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) cursor.getString(0) else null
            }
        }.getOrNull()
        return provided?.takeIf { it.isNotBlank() } ?: uri.lastPathSegment?.substringAfterLast('/') ?: "Document.pdf"
    }

    /**
     * Describes a URI picked with `CreateDocument` as a document the app is
     * about to reopen, keeping access so **Save** can write back to it.
     */
    fun created(resolver: ContentResolver, uri: Uri): CreatedDocument =
        CreatedDocument(displayName(resolver, uri), persistAccess(resolver, uri).saveTarget)

    /** Writes [bytes] to a URI the user picked with `CreateDocument`. */
    fun writeCopy(resolver: ContentResolver, uri: Uri, bytes: ByteArray): Boolean = write(resolver, uri, bytes)

    /** Writes [bytes] back over the document a save target came from. */
    fun writeBack(resolver: ContentResolver, saveTarget: String, bytes: ByteArray): Boolean = write(resolver, Uri.parse(saveTarget), bytes)

    /** What [persistAccess] kept: a save target when a write grant persisted, and whether a read grant did. */
    private class PersistedAccess(val saveTarget: String?, val readable: Boolean)

    /**
     * Keeps access to [uri] across process death and returns it as a save
     * target when the provider will accept a write. Write access is only
     * requested when the provider advertises it: asking for a flag the picker
     * never granted throws, and would cost us the read grant too.
     */
    private fun persistAccess(resolver: ContentResolver, uri: Uri): PersistedAccess {
        val writable = supportsWrite(resolver, uri)
        val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION or if (writable) Intent.FLAG_GRANT_WRITE_URI_PERMISSION else 0
        val persisted = runCatching { resolver.takePersistableUriPermission(uri, flags) }.isSuccess
        val readable = persisted || runCatching { resolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION) }.isSuccess
        return PersistedAccess(uri.toString().takeIf { writable && persisted }, readable)
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
