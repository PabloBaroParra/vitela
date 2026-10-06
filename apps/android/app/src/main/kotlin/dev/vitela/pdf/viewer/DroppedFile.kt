package dev.vitela.pdf.viewer

internal const val IMAGE_DROPPED_PLACED = "Dropped image placed."
internal const val DROP_NOT_ANNOTATABLE = "This document does not allow annotation edits."
internal const val DROP_UNSUPPORTED = "Vitela opens PDF files. Drop a PNG or JPEG onto a page to add it as a stamp."
internal const val DROP_UNREADABLE = "The dropped file could not be read."
internal const val DROP_IMAGE_OFF_PAGE = "Drop the image onto a page to place it as a stamp."
internal const val DROP_IMAGE_WITHOUT_DOCUMENT = "Open a PDF first, then drop an image onto a page to stamp it."

private const val PDF_MIME = "application/pdf"

/**
 * What a file dragged in from another app (split screen, freeform windows)
 * means here — the same meanings a desktop drop has on Linux and Windows. A
 * PDF opens as the document, through the same unsaved-changes guard as the
 * picker; an image dropped on a page becomes a stamp at the drop point.
 */
internal sealed interface DroppedFile {
    data class Pdf(val uri: String) : DroppedFile
    data class Image(val uri: String) : DroppedFile
    data object Unsupported : DroppedFile
}

/** Whether a drag is worth lighting a target up for, from its clip description's MIME types. */
internal fun acceptsDrop(mimeTypes: List<String>): Boolean = mimeTypes.any { it == PDF_MIME || it.startsWith("image/") }

/**
 * The first file a drop carries, or `null` when it carries none the shell may
 * read. Only a `content:` URI qualifies — the rule paste and "Open with"
 * follow — so a dragged link never becomes a download. Like the desktop
 * shells, only the first file is acted on.
 *
 * [mimeTypeOf] is the provider's type for that one URI, asked only once the
 * URI has qualified. Takes plain values, so the Android `DragEvent` stays in
 * the activity.
 */
internal fun droppedFile(itemUris: List<String?>, mimeTypeOf: (String) -> String?): DroppedFile? {
    val uri = itemUris.firstOrNull { it != null && it.substringBefore(':', "").equals("content", ignoreCase = true) } ?: return null
    val type = mimeTypeOf(uri)
    return when {
        type == PDF_MIME -> DroppedFile.Pdf(uri)
        type?.startsWith("image/") == true -> DroppedFile.Image(uri)
        else -> DroppedFile.Unsupported
    }
}
