package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.WatermarkLook

/** Which pages the Add watermark dialog stamps. */
enum class WatermarkPages { All, Current, Custom }

/**
 * What the Add watermark dialog shows: the text, the pages and the look.
 * Opacity is asked in percent, the way every image editor asks it.
 */
data class WatermarkDraft(
    val text: String = WATERMARK_DEFAULT_TEXT,
    val pages: WatermarkPages = WatermarkPages.All,
    /** The one-based range typed for [WatermarkPages.Custom], such as `"1-3,7"`. */
    val range: String = "",
    val fontSizePt: Double,
    val opacityPercent: Double,
    /** Counter-clockwise, as the reader sees the page. */
    val rotationDegrees: Double,
) {
    val look get() = WatermarkLook(fontSizePt, opacityPercent / 100, rotationDegrees)

    companion object {
        /** A fresh draft drawn as [look] — the core's default, so every shell's first stamp is the same. */
        fun from(look: WatermarkLook) = WatermarkDraft(fontSizePt = look.fontSizePt, opacityPercent = look.opacity * 100, rotationDegrees = look.rotationDegrees)
    }
}

/**
 * The open Add watermark dialog. [currentPage] is the page the reader was on
 * when it opened — what "Current page" means. [message] is why the last Add
 * was refused; the draft is kept so nothing typed is lost.
 */
data class WatermarkEditor(val draft: WatermarkDraft, val currentPage: Int, val message: String? = null)

/**
 * The dialog's ranges — the GTK and Windows shells' own, so the three refuse
 * the same values. The size floor keeps the stamp legible and its ceiling is
 * past what fits across portrait A4 at 45°; the opacity floor is 5, not 1,
 * because the core refuses only a fully transparent stamp but 1% is one
 * nobody will ever find.
 */
internal val WATERMARK_FONT_SIZE_RANGE = 8.0..200.0
internal val WATERMARK_OPACITY_PERCENT_RANGE = 5.0..100.0
internal val WATERMARK_ROTATION_RANGE = -180.0..180.0

internal const val WATERMARK_DEFAULT_TEXT = "DRAFT"

/**
 * The pages [draft] stamps, as zero-based positions, or the sentence to show
 * instead. The text is checked first — kept as typed, leading spaces and all,
 * since the stamp is centred on its own width — and only an all-blank one is
 * refused. A typed range is read by the core's grammar, the one Export images
 * and Extract pages share.
 *
 * Blocking (it crosses the FFI); callers run it off the main thread.
 */
internal fun planWatermark(document: PdfDocument, draft: WatermarkDraft, currentPage: Int): PdfCoreResult<List<Int>> {
    if (draft.text.isBlank()) return refusal("Type the text to stamp on the pages.")
    val total = document.pageCount
    if (total == 0) return refusal("This document has no pages to stamp.")
    return when (draft.pages) {
        WatermarkPages.All -> PdfCoreResult.Success((0 until total).toList())
        // Clamped: the position readout can sit one past the end right after a deletion.
        WatermarkPages.Current -> PdfCoreResult.Success(listOf(currentPage.coerceIn(0, total - 1)))
        WatermarkPages.Custom ->
            if (draft.range.isBlank()) refusal("Type which pages to stamp, for example 1-3,7.")
            else document.parsePageSelection(draft.range)
    }
}

/** What the status line says once the stamp is recorded. */
internal fun watermarkSummary(count: Int) = "Watermark added to $count ${if (count == 1) "page" else "pages"}."

private fun refusal(message: String) = PdfCoreResult.Failure(PdfCoreError.Failed(message))
