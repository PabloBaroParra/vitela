package dev.vitela.pdf.core

/**
 * The editable text keys of the PDF's Document Info Dictionary. `null` means
 * the key is absent from `/Info`, never "present and empty". The two dates are
 * left out on purpose: they are not user-editable, and the adapter carries the
 * file's own values through every write.
 */
data class DocumentInfo(
    val title: String? = null,
    val author: String? = null,
    val subject: String? = null,
    val keywords: String? = null,
    val creator: String? = null,
    val producer: String? = null,
)
