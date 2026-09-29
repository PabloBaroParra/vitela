package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.ImageExportFormat
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument

/** Which pages an export covers. */
enum class ImageExportPages { All, Current, Custom }

/**
 * What the Export images dialog shows. [dpi] and [customRange] stay text so an
 * edit survives recomposition as typed; [planImageExport] is what reads them.
 */
data class ImageExportDraft(
    val pages: ImageExportPages = ImageExportPages.All,
    val customRange: String = "",
    val dpi: String = DEFAULT_EXPORT_DPI.toString(),
    val format: ImageExportFormat = ImageExportFormat.Png,
)

/** The open Export images dialog: the [draft], whether the file lets pages leave, and the reason for the last refusal. */
data class ImageExportEditor(
    val draft: ImageExportDraft,
    val exportAllowed: Boolean,
    val message: String? = null,
)

/** One file an export writes: the page, and the name the core gave it. */
data class ImageExportFile(val pageIndex: Int, val fileName: String)

/**
 * An export every answer of which has been checked — the pages, their names,
 * the resolution — so only the disk or the renderer can still refuse it.
 */
data class ImageExportPlan(val files: List<ImageExportFile>, val dpi: Int, val format: ImageExportFormat)

/**
 * The ceiling is 400, not the 600 an archival scan would suggest: the core
 * refuses any raster over 32 Mpx, and US Letter at 600 DPI is 33.7 Mpx. A4
 * misses too, so a higher notch would be a control that lies. The Windows and
 * GTK shells stop at the same place for the same reason.
 */
internal const val MIN_EXPORT_DPI = 72
internal const val MAX_EXPORT_DPI = 400
internal const val DEFAULT_EXPORT_DPI = 150

internal const val IMAGE_EXPORT_NOT_ALLOWED = "This document does not permit extracting its pages as images."

/**
 * Checks every choice in [draft] against [document] and returns the files an
 * export would write, or the reason it cannot. Nothing here is the shell's own
 * rule: the range grammar, the names and the raster ceiling are the core's, so
 * this shell cannot disagree with the others about what `"7-3"` means.
 *
 * Blocking (it crosses the FFI); callers run it off the main thread.
 */
internal fun planImageExport(
    document: PdfDocument,
    displayName: String,
    currentPage: Int,
    draft: ImageExportDraft,
): PdfCoreResult<ImageExportPlan> {
    // A cleared box reads as the default, like the Windows NumberBox; anything
    // typed that is not a number in range is refused, never rounded into one.
    val dpi = if (draft.dpi.isBlank()) DEFAULT_EXPORT_DPI else draft.dpi.trim().toIntOrNull()
    if (dpi == null || dpi !in MIN_EXPORT_DPI..MAX_EXPORT_DPI) {
        return refusal("Choose a resolution between $MIN_EXPORT_DPI and $MAX_EXPORT_DPI DPI.")
    }
    if (!document.imageExportAllowed()) return refusal(IMAGE_EXPORT_NOT_ALLOWED)
    val total = document.pageCount
    if (total == 0) return refusal("This document has no pages to export.")

    val pages = when (draft.pages) {
        ImageExportPages.All -> List(total) { it }
        // Clamped rather than trusted: a page removal can leave the reader one
        // past the end until it catches up.
        ImageExportPages.Current -> listOf(currentPage.coerceIn(0, total - 1))
        ImageExportPages.Custom -> when (val parsed = document.parsePageSelection(draft.customRange)) {
            is PdfCoreResult.Success -> parsed.value
            is PdfCoreResult.Failure -> return parsed
        }
    }

    when (val oversized = document.firstPageTooLargeToExport(pages, dpi)) {
        is PdfCoreResult.Failure -> return oversized
        is PdfCoreResult.Success -> oversized.value?.let {
            return refusal("Page ${it + 1} is too large to export at $dpi DPI. Choose a lower resolution.")
        }
    }

    val files = pages.map { page ->
        when (val name = document.pageImageFileName(displayName, page, draft.format)) {
            is PdfCoreResult.Success -> ImageExportFile(page, name.value)
            is PdfCoreResult.Failure -> return name
        }
    }
    return PdfCoreResult.Success(ImageExportPlan(files, dpi, draft.format))
}

private fun refusal(message: String) = PdfCoreResult.Failure(PdfCoreError.Failed(message))

internal fun imageExportSummary(count: Int, format: ImageExportFormat): String =
    "Exported $count ${if (count == 1) "page" else "pages"} as ${format.label}."
