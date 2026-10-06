package dev.vitela.pdf.document

/**
 * The PDF another app handed over, or `null` when the launch carries none.
 *
 * "Open with" (`ACTION_VIEW`) puts the document in the intent's data; the
 * share sheet (`ACTION_SEND`) puts it in `EXTRA_STREAM`. Either way only a
 * `content:` URI qualifies — local content the resolver reads. A remote URL
 * would make opening a download, and a `file:` path bypasses the Storage
 * Access Framework the core's bytes-only contract relies on.
 *
 * Takes the intent's pieces as plain values, so the Android `Intent` stays in
 * the activity.
 */
internal fun openablePdfUri(action: String?, data: String?, stream: String?): String? {
    val uri = when (action) {
        "android.intent.action.VIEW" -> data
        "android.intent.action.SEND" -> stream
        else -> null
    }
    return uri?.takeIf { it.substringBefore(':', "").equals("content", ignoreCase = true) }
}
