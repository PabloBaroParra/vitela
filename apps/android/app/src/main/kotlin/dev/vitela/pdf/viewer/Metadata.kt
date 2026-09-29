package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.DocumentInfo

/**
 * The open Document properties dialog. [draft] is what the text boxes show,
 * so an edit survives recomposition and a refused apply keeps what was typed.
 */
data class MetadataEditor(
    val draft: DocumentInfo,
    val editingAllowed: Boolean,
    /** Why the properties are read-only, or why the last apply was refused. */
    val message: String? = null,
)

internal const val METADATA_READ_ONLY = "This document does not permit metadata changes."

/**
 * What an apply should queue, or null when [draft] says nothing new. A text
 * box cannot tell "cleared" from "never had a value", so an empty field is
 * read as an absent key — the core's `null` — and never written as `""`.
 */
internal fun metadataChange(current: DocumentInfo, draft: DocumentInfo): DocumentInfo? {
    val after = DocumentInfo(
        title = draft.title.emptyToNull(),
        author = draft.author.emptyToNull(),
        subject = draft.subject.emptyToNull(),
        keywords = draft.keywords.emptyToNull(),
        creator = draft.creator.emptyToNull(),
        producer = draft.producer.emptyToNull(),
    )
    return after.takeIf { it != current }
}

private fun String?.emptyToNull(): String? = if (isNullOrEmpty()) null else this
