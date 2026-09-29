package dev.vitela.pdf.document

import android.content.ContentResolver
import android.net.Uri
import android.provider.DocumentsContract

/**
 * Storage Access Framework I/O for exported page images. [treeUri] is the
 * folder the user picked with `OpenDocumentTree`; nothing above this layer
 * knows a [Uri].
 */
object SafExport {
    /**
     * Creates [fileName] in the picked folder and writes [bytes] into it,
     * returning false when it could not. A file this call started but could
     * not finish is deleted, so a failed export leaves no truncated image
     * behind. Name collisions are the provider's to resolve — it renames the
     * new document instead of overwriting an earlier export.
     */
    fun writeFile(resolver: ContentResolver, treeUri: Uri, fileName: String, mimeType: String, bytes: ByteArray): Boolean {
        val created = runCatching {
            val folder = DocumentsContract.buildDocumentUriUsingTree(treeUri, DocumentsContract.getTreeDocumentId(treeUri))
            DocumentsContract.createDocument(resolver, folder, mimeType, fileName)
        }.getOrNull() ?: return false
        val written = runCatching {
            resolver.openOutputStream(created, "wt")?.use { it.write(bytes) } ?: error("No output stream")
        }.isSuccess
        if (!written) runCatching { DocumentsContract.deleteDocument(resolver, created) }
        return written
    }
}
