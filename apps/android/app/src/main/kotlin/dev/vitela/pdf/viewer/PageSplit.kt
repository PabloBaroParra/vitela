package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.SplitPart

/** The open Split dialog: the typed [cuts], whether the file lets pages leave, and the reason for the last refusal. */
data class PageSplitEditor(
    val cuts: String,
    val splitAllowed: Boolean,
    val message: String? = null,
)

/** A split whose cuts have been checked, so only the core's rewrite or the disk can still refuse a part. */
data class PageSplitPlan(val parts: List<SplitPart>, val sourceIsSigned: Boolean)

internal const val PAGE_SPLIT_NO_REWRITE =
    "Splitting pages rewrites the whole file, which this document's encryption or available credentials do not allow."

/**
 * Why [document] cannot be split at all, or null when it can. Each part is an
 * extraction, so these are Extract pages' two gates in the same order — and
 * the Windows split plan's.
 */
internal fun pageSplitRefusal(document: PdfDocument): String? = when {
    !document.pageExtractionAllowed() -> PAGE_EXTRACT_NOT_ALLOWED
    !document.fullRewriteAllowed() -> PAGE_SPLIT_NO_REWRITE
    else -> null
}

/**
 * Checks [cuts] against [document] and returns the parts a split would write,
 * named after [title], or the reason it cannot. The cut grammar, the one-page
 * refusal and the part names are the core's, so `"3,7"` makes the same files
 * here as on Linux and Windows.
 *
 * Blocking (it crosses the FFI); callers run it off the main thread.
 */
internal fun planPageSplit(document: PdfDocument, title: String, cuts: String): PdfCoreResult<PageSplitPlan> {
    pageSplitRefusal(document)?.let { return PdfCoreResult.Failure(PdfCoreError.Failed(it)) }
    return when (val planned = document.planSplit(cuts, title)) {
        is PdfCoreResult.Success -> PdfCoreResult.Success(PageSplitPlan(planned.value, document.extractSourceIsSigned()))
        is PdfCoreResult.Failure -> planned
    }
}

/** The Windows summary without its destination: a SAF folder has no path to show. */
internal fun pageSplitSummary(count: Int, sourceIsSigned: Boolean): String {
    val summary = "Split into $count PDFs."
    return if (sourceIsSigned) "$summary The original document is signed, so the signature in each part no longer verifies." else summary
}
