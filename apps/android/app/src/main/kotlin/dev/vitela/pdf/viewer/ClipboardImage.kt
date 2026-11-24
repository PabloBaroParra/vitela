package dev.vitela.pdf.viewer

import dev.vitela.pdf.R
import dev.vitela.pdf.text.UiText

internal val PASTE_STAMP_PROMPT: UiText = UiText.Resource(R.string.status_stamp_paste_prompt)
internal val CLIPBOARD_HAS_NO_IMAGE: UiText = UiText.Resource(R.string.status_stamp_clipboard_no_image)
internal val CLIPBOARD_IMAGE_UNREADABLE: UiText = UiText.Resource(R.string.status_stamp_clipboard_unreadable)

/**
 * The clipboard item a paste may read, or `null` when it offers no image.
 *
 * Only an image type backed by a `content:` URI qualifies — a local provider
 * the content resolver reads. Clipboard text is never read, so a copied URL
 * cannot become an implicit download (T-091, as on Linux and Windows); an
 * image type over any other scheme is refused for the same reason.
 *
 * Takes the clip's MIME types and its items' URIs as plain values, so the
 * Android `ClipData` stays in the activity.
 */
internal fun pastableImageUri(mimeTypes: List<String>, itemUris: List<String?>): String? {
    if (mimeTypes.none { it.startsWith("image/") }) return null
    return itemUris.firstOrNull { it != null && it.substringBefore(':', "").equals("content", ignoreCase = true) }
}
