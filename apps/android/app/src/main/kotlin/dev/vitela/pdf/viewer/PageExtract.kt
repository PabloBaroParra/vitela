package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument

/** The open Extract pages dialog: the typed [range], whether the file lets pages leave, and the reason for the last refusal. */
data class PageExtractEditor(
    val range: String,
    val extractAllowed: Boolean,
    val message: String? = null,
)

/** An extraction whose range has been checked, so only the core's rewrite or the disk can still refuse it. */
data class PageExtractPlan(val pages: List<Int>, val sourceIsSigned: Boolean)

internal const val PAGE_EXTRACT_NOT_ALLOWED = "This document does not permit extracting its pages."
internal const val PAGE_EXTRACT_NO_REWRITE =
    "Extracting pages rewrites the whole file, which this document's encryption or available credentials do not allow."

/** Why [document] cannot have pages extracted at all, or null when it can. The same two gates, in the same order, as the Windows plan. */
internal fun pageExtractRefusal(document: PdfDocument): String? = when {
    // The extraction bit, deliberately not page assembly: an extraction
    // changes nothing about the open document (see pdf-ffi's extract module).
    !document.pageExtractionAllowed() -> PAGE_EXTRACT_NOT_ALLOWED
    !document.fullRewriteAllowed() -> PAGE_EXTRACT_NO_REWRITE
    else -> null
}

/**
 * Checks [range] against [document] and returns the pages an extraction would
 * keep, or the reason it cannot. The grammar is the core's, shared with the
 * image export, so `"7-3"` means the same thing here as on every other shell.
 *
 * Blocking (it crosses the FFI); callers run it off the main thread.
 */
internal fun planPageExtract(document: PdfDocument, range: String): PdfCoreResult<PageExtractPlan> {
    pageExtractRefusal(document)?.let { return refusal(it) }
    if (document.pageCount == 0) return refusal("This document has no pages to extract.")
    if (range.isBlank()) return refusal("Type which pages to extract, for example 1-3,7.")
    return when (val parsed = document.parsePageSelection(range)) {
        is PdfCoreResult.Success -> PdfCoreResult.Success(PageExtractPlan(parsed.value, document.extractSourceIsSigned()))
        is PdfCoreResult.Failure -> parsed
    }
}

private fun refusal(message: String) = PdfCoreResult.Failure(PdfCoreError.Failed(message))

/** The name the save picker suggests: the open file's stem marked as an extract, as on Windows. */
internal fun extractFileName(title: String): String = "${documentStem(title)}-extract.pdf"

/**
 * The Windows summary without its destination: a SAF document has no path to
 * show, only whatever name the provider settled on.
 */
internal fun pageExtractSummary(count: Int, sourceIsSigned: Boolean): String {
    val summary = "Extracted $count ${if (count == 1) "page" else "pages"} to a new PDF."
    return if (sourceIsSigned) "$summary The original document is signed, so the extracted PDF's signature no longer verifies." else summary
}
