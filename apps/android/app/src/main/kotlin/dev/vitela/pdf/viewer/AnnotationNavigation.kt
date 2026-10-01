package dev.vitela.pdf.viewer

import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.lazy.LazyListState
import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.PageSize
import kotlin.math.roundToInt

/**
 * An annotation the reader should scroll into view — set by Previous/Next
 * annotation, cleared once the list has consumed it, like [ViewerState.scrollTarget].
 * [bounds] is in the page's unrotated space; the list places it through the
 * page's turn and its current zoom, which only the list knows.
 */
data class AnnotationReveal(val pageIndex: Int, val bounds: AnnotationRect)

/** Where the list scrolls to show a placed rect: an offset into the page's slot, and the column's horizontal scroll. */
internal data class RevealScroll(val itemOffsetPx: Int, val scrollXPx: Int)

/**
 * Navigation needs annotations to visit and the reader to show them. Unlike
 * the annotation tools it is not gated on editing: looking at an annotation
 * changes nothing. The grid hides the reader, and Edit content claims the
 * page's taps and drops the annotation selection on its way in.
 */
internal fun annotationNavigationEnabled(state: ViewerState): Boolean =
    state.annotations.isNotEmpty() && !state.isLoading && state.organize == null && state.contentEdit == null

/**
 * The selected annotation when **Read note** can show it: a note, while the
 * reader is showing. Gated like navigation, not like editing — reading a note
 * changes nothing, so a document that forbids annotating can still be read.
 */
internal fun readableNote(state: ViewerState): Annotation? =
    state.annotations.lastOrNull { it.id == state.selectedAnnotationId }
        ?.takeIf { it.kind == AnnotationKind.TextNote && annotationNavigationEnabled(state) }

/**
 * The annotation after (or before) [selectedId] in snapshot order, wrapping at
 * both ends. Without a selection, forward enters at the first and backward at
 * the last. A selection the snapshot no longer has goes nowhere. Parity with
 * the Windows shell's `AnnotationSelection`.
 */
internal fun annotationStep(annotations: List<Annotation>, selectedId: Long?, forward: Boolean): Annotation? {
    if (annotations.isEmpty()) return null
    if (selectedId == null) return if (forward) annotations.first() else annotations.last()
    val index = annotations.indexOfFirst { it.id == selectedId }
    if (index < 0) return null
    return annotations[(index + if (forward) 1 else annotations.size - 1) % annotations.size]
}

/** What to scroll to for [annotation]: its rect, or an ink stroke's bounds. */
internal fun annotationReveal(annotation: Annotation): AnnotationReveal? =
    annotation.bounds?.let { AnnotationReveal(annotation.pageIndex, it) }

/**
 * Centres [placed] in the viewport, or shows its top-left corner when it does
 * not fit. Never scrolls above the page's top or left of the column; the list
 * and the scroll state clamp the far ends themselves.
 */
internal fun revealScroll(placed: PlacedRect, viewportWidthPx: Int, viewportHeightPx: Int): RevealScroll {
    fun start(near: Double, extent: Double, viewport: Int) =
        (if (extent > viewport) near else near + extent / 2 - viewport / 2.0).roundToInt().coerceAtLeast(0)
    return RevealScroll(
        itemOffsetPx = start(placed.top, placed.height, viewportHeightPx),
        scrollXPx = start(placed.left, placed.width, viewportWidthPx),
    )
}

/** The reader's geometry a reveal is placed against: the page column's width, and what is on screen. */
internal data class RevealViewport(val pageWidthPx: Int, val widthPx: Int, val heightPx: Int)

/**
 * Scrolls the reader so [reveal] is on screen, through the page's turn and the
 * current zoom. Without a laid-out column or a page size it falls back to the
 * page's top, like a search hit.
 */
internal suspend fun revealAnnotation(
    reveal: AnnotationReveal,
    size: PageSize?,
    viewport: RevealViewport,
    listState: LazyListState,
    horizontalScrollState: ScrollState,
) {
    if (size == null || viewport.pageWidthPx <= 0) {
        listState.animateScrollToItem(reveal.pageIndex)
        return
    }
    // The slot fills the column, so the column's width is the page's drawn width.
    val placed = PagePlacement(size, viewport.pageWidthPx.toDouble() / size.widthPt).placeRect(reveal.bounds)
    val scroll = revealScroll(placed, viewport.widthPx, viewport.heightPx)
    listState.animateScrollToItem(reveal.pageIndex, scroll.itemOffsetPx)
    horizontalScrollState.animateScrollTo(scroll.scrollXPx)
}
