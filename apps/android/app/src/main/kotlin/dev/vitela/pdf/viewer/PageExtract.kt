package dev.vitela.pdf.viewer

import androidx.annotation.StringRes
import dev.vitela.pdf.R
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.text.UiText

/** The open Extract pages dialog: the typed [range], whether the file lets pages leave, and the reason for the last refusal. */
data class PageExtractEditor(
    val range: String,
    val extractAllowed: Boolean,
    val message: UiText? = null,
)

/** An extraction whose range has been checked, so only the core's rewrite or the disk can still refuse it. */
data class PageExtractPlan(val pages: List<Int>, val sourceIsSigned: Boolean)

internal val PAGE_EXTRACT_NOT_ALLOWED: UiText = UiText.Resource(R.string.error_extract_forbidden)
internal val PAGE_EXTRACT_NO_REWRITE: UiText = UiText.Resource(R.string.error_extract_no_rewrite)

/** Why [document] cannot have pages extracted at all, or null when it can. The same two gates, in the same order, as the Windows plan. */
internal fun pageExtractRefusal(document: PdfDocument): UiText? = when {
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
    pageExtractRefusal(document)?.let { return PdfCoreResult.Failure(PdfCoreError.Failed(it)) }
    if (document.pageCount == 0) return refusal(R.string.error_extract_no_pages)
    if (range.isBlank()) return refusal(R.string.error_extract_range_missing)
    return when (val parsed = document.parsePageSelection(range)) {
        is PdfCoreResult.Success -> PdfCoreResult.Success(PageExtractPlan(parsed.value, document.extractSourceIsSigned()))
        is PdfCoreResult.Failure -> parsed
    }
}

private fun refusal(@StringRes id: Int) = PdfCoreResult.Failure(PdfCoreError.Failed(id))

/** The name the save picker suggests: the open file's stem marked as an extract, as on Windows. */
internal fun extractFileName(title: String): String = "${documentStem(title)}-extract.pdf"

/**
 * The Windows summary without its destination: a SAF document has no path to
 * show, only whatever name the provider settled on.
 */
internal fun pageExtractSummary(count: Int, sourceIsSigned: Boolean): UiText {
    val summary = UiText.Plural(R.plurals.status_extract_done, count, count)
    return if (sourceIsSigned) UiText.Sentences(summary, UiText.Resource(R.string.status_extract_signed)) else summary
}
