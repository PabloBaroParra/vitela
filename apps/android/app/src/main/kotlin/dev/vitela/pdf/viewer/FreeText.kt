package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.FreeTextLine
import dev.vitela.pdf.core.PageSize

/** A new text box's size, in points, before it is clamped to the page. */
internal const val FREE_TEXT_WIDTH_PT = 200.0
internal const val FREE_TEXT_HEIGHT_PT = 50.0

/**
 * The smallest box the core accepts for the default 12 pt Helvetica: one glyph
 * wide and one line high, padding included (`min_free_text_size`). A drag
 * below it is clamped here, so the core is never asked for a degenerate rect.
 */
internal const val FREE_TEXT_MIN_WIDTH_PT = 16.0
internal const val FREE_TEXT_MIN_HEIGHT_PT = 17.8

internal const val FREE_TEXT_ADDED = "Text box added. Save to keep it."
internal const val FREE_TEXT_EDITED = "Text updated. Save to keep the change."
internal const val FREE_TEXT_UNCHANGED = "Text unchanged."
internal const val FREE_TEXT_CANCELED = "Text box canceled."
internal const val ANNOTATION_CHANGED_UNDER_EDITOR = "The text box changed while the dialog was open. Nothing was changed."

/** What the text dialog is for: a box about to be placed, or the text of one already there. */
sealed interface FreeTextTarget {
    /** A new box at [rect], chosen by the tap; nothing reaches the core until **Add**. */
    data class Place(val rect: AnnotationRect) : FreeTextTarget

    /** The text of the box [annotationId], as it was when the dialog opened. */
    data class Retype(val annotationId: Long, val contents: String) : FreeTextTarget
}

/**
 * The text dialog's state. [error] is the core's refusal, shown with what the
 * reader typed still in the field; [busy] is true from the moment the core is
 * asked until it answers, so a second tap on the button sends nothing.
 */
data class FreeTextDraft(
    val pageIndex: Int,
    val target: FreeTextTarget,
    val error: String? = null,
    val busy: Boolean = false,
)

/**
 * A new box with its top-left corner on [tap] (PDF space, y up), [FREE_TEXT_WIDTH_PT]
 * by [FREE_TEXT_HEIGHT_PT] and kept wholly on the page. A box lives in the page's
 * unrotated space, so a turned page is clamped against its [PageSize.unrotated] size.
 */
internal fun freeTextPlacementRect(tap: AnnotationPoint, page: PageSize): AnnotationRect {
    val space = page.unrotated
    val width = minOf(FREE_TEXT_WIDTH_PT, space.widthPt)
    val height = minOf(FREE_TEXT_HEIGHT_PT, space.heightPt)
    val x = tap.x.coerceIn(0.0, space.widthPt - width)
    val y = (tap.y - height).coerceIn(0.0, space.heightPt - height)
    return AnnotationRect(x, y, width, height)
}

/**
 * The box's own frame (x right, y down, in points) as an affine map onto the
 * page as drawn: `(x, y)` lands at `(a·x + c·y + e, b·x + d·y + f)` in pixels.
 * It carries the page's turn and zoom, so the lines are drawn once, upright in
 * their own frame, wherever the page lies.
 */
internal data class FreeTextFrame(val a: Double, val b: Double, val c: Double, val d: Double, val e: Double, val f: Double)

/** What the overlay paints for a text box: the core's [lines] in [frame], clipped to [width] by [height]. */
internal data class FreeTextDrawing(
    val frame: FreeTextFrame,
    val width: Double,
    val height: Double,
    val fontSizePt: Double,
    val lines: List<FreeTextLine>,
)

/**
 * The drawing of [annotation] under [placement], or null for anything that is
 * not a text box the core laid out. The corners are placed as page-space
 * points, the way a stamp's picture is, so a `/Rotate` turns the text with the
 * page; the lines are never re-wrapped here.
 */
internal fun freeTextDrawing(annotation: Annotation, placement: PagePlacement): FreeTextDrawing? {
    val layout = annotation.layout ?: return null
    val rect = annotation.rect ?: return null
    if (annotation.kind != AnnotationKind.FreeText) return null
    val top = rect.y + rect.height
    val topLeft = placement.placePoint(AnnotationPoint(rect.x, top))
    val topRight = placement.placePoint(AnnotationPoint(rect.x + rect.width, top))
    val bottomLeft = placement.placePoint(AnnotationPoint(rect.x, rect.y))
    // `+ 0.0` turns a -0.0 into 0.0, so a frame compares equal however it was reached.
    val frame = FreeTextFrame(
        (topRight.x - topLeft.x) / rect.width + 0.0, (topRight.y - topLeft.y) / rect.width + 0.0,
        (bottomLeft.x - topLeft.x) / rect.height + 0.0, (bottomLeft.y - topLeft.y) / rect.height + 0.0,
        topLeft.x + 0.0, topLeft.y + 0.0,
    )
    return FreeTextDrawing(frame, rect.width, rect.height, layout.fontSizePt, layout.lines)
}

/** **Add** / **Save** stays disabled while the text is blank, and while the core is still answering. */
internal fun freeTextConfirmEnabled(text: String, busy: Boolean): Boolean = text.isNotBlank() && !busy

internal fun freeTextDialogTitle(draft: FreeTextDraft): String =
    (if (draft.target is FreeTextTarget.Place) "Add text box" else "Edit text") + " — page ${draft.pageIndex + 1}"

internal fun freeTextConfirmLabel(draft: FreeTextDraft): String = if (draft.target is FreeTextTarget.Place) "Add" else "Save"

internal fun freeTextInitialText(draft: FreeTextDraft): String = (draft.target as? FreeTextTarget.Retype)?.contents.orEmpty()
