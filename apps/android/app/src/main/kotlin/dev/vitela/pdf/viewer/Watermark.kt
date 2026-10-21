package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.PictureWatermarkLook
import dev.vitela.pdf.core.WatermarkLayer
import dev.vitela.pdf.core.WatermarkLook

/** Which pages the Add watermark dialog stamps. */
enum class WatermarkPages { All, Current, Custom }

/** What the Add watermark dialog paints: the same text, or the same picture, on every page. */
enum class WatermarkKind { Text, Picture }

/**
 * A PNG or JPEG exactly as the reader picked it — decoding is the core's job,
 * and it is handed these bytes untouched. Not a data class: two picks are two
 * pictures, compared by identity, and a byte array has no useful `equals`.
 */
class WatermarkPicture(val bytes: ByteArray) {
    /** What the dialog says under Choose picture. */
    val summary get() = "Picture chosen (${maxOf(1, bytes.size / 1024)} KB)."
}

/**
 * What the Add watermark dialog shows: the kind, the text or picture, the
 * pages and the look. Both kinds share the pages, opacity, angle and layer;
 * the text's size is in points and the picture's in percent of the page.
 * Opacity is asked in percent, the way every image editor asks it.
 */
data class WatermarkDraft(
    val kind: WatermarkKind = WatermarkKind.Text,
    val text: String = WATERMARK_DEFAULT_TEXT,
    val picture: WatermarkPicture? = null,
    val pages: WatermarkPages = WatermarkPages.All,
    /** The one-based range typed for [WatermarkPages.Custom], such as `"1-3,7"`. */
    val range: String = "",
    val fontSizePt: Double,
    val scalePercent: Double,
    val opacityPercent: Double,
    /** Counter-clockwise, as the reader sees the page. */
    val rotationDegrees: Double,
    val layer: WatermarkLayer,
) {
    val look get() = WatermarkLook(fontSizePt, opacityPercent / 100, rotationDegrees, layer)
    val pictureLook get() = PictureWatermarkLook(scalePercent / 100, opacityPercent / 100, rotationDegrees, layer)

    /**
     * This draft showing [kind]. Switching resets the angle and the layer to
     * the core's defaults for that kind, as the GTK and Windows dialogs do: a
     * logo left at text's 45° reads as a mistake, and a background picture
     * moved over the text hides the page. Everything else is kept.
     */
    fun switchedTo(kind: WatermarkKind, defaults: WatermarkDefaults): WatermarkDraft {
        if (kind == this.kind) return this
        return when (kind) {
            WatermarkKind.Text -> copy(kind = kind, rotationDegrees = defaults.text.rotationDegrees, layer = defaults.text.layer)
            WatermarkKind.Picture -> copy(kind = kind, rotationDegrees = defaults.picture.rotationDegrees, layer = defaults.picture.layer)
        }
    }
}

/** The core's starting looks — diagonal text over the content, an upright picture behind it — so every shell's first stamp is the same. */
data class WatermarkDefaults(val text: WatermarkLook, val picture: PictureWatermarkLook) {
    /** A fresh draft: text, drawn the core's way, with the picture's size ready for a switch. */
    fun draft() = WatermarkDraft(
        fontSizePt = text.fontSizePt,
        scalePercent = picture.scale * 100,
        opacityPercent = text.opacity * 100,
        rotationDegrees = text.rotationDegrees,
        layer = text.layer,
    )
}

/**
 * The open Add watermark dialog. [currentPage] is the page the reader was on
 * when it opened — what "Current page" means. [message] is why the last Add
 * was refused; the draft is kept so nothing typed is lost.
 */
data class WatermarkEditor(val draft: WatermarkDraft, val defaults: WatermarkDefaults, val currentPage: Int, val message: String? = null) {
    fun switchedTo(kind: WatermarkKind) = draft.switchedTo(kind, defaults)
}

/**
 * The dialog's ranges — the GTK and Windows shells' own, so the three refuse
 * the same values. The size floor keeps the stamp legible and its ceiling is
 * past what fits across portrait A4 at 45°; the opacity floor is 5, not 1,
 * because the core refuses only a fully transparent stamp but 1% is one
 * nobody will ever find.
 */
internal val WATERMARK_FONT_SIZE_RANGE = 8.0..200.0
internal val WATERMARK_OPACITY_PERCENT_RANGE = 5.0..100.0

/** A picture's share of the page: the same "nobody would find it" floor; 100 touches two opposite edges. */
internal val WATERMARK_SCALE_PERCENT_RANGE = 5.0..100.0
internal val WATERMARK_ROTATION_RANGE = -180.0..180.0

internal const val WATERMARK_DEFAULT_TEXT = "DRAFT"

/**
 * The pages [draft] stamps, as zero-based positions, or the sentence to show
 * instead. The mark is checked first: a text — kept as typed, leading spaces
 * and all, since the stamp is centred on its own width — is refused only when
 * all blank, and a picture only when none was chosen; whether its bytes
 * decode is the core's call when it records. A typed range is read by the core's grammar, the one Export images
 * and Extract pages share.
 *
 * Blocking (it crosses the FFI); callers run it off the main thread.
 */
internal fun planWatermark(document: PdfDocument, draft: WatermarkDraft, currentPage: Int): PdfCoreResult<List<Int>> {
    when (draft.kind) {
        WatermarkKind.Text -> if (draft.text.isBlank()) return refusal("Type the text to stamp on the pages.")
        WatermarkKind.Picture -> if (draft.picture == null) return refusal("Choose a PNG or JPEG picture to stamp on the pages.")
    }
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
