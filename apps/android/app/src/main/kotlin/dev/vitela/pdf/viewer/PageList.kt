package dev.vitela.pdf.viewer

import androidx.compose.foundation.Image
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.awaitTouchSlopOrCancellation
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.withFrameNanos
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.draganddrop.DragAndDropEvent
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.AnnotationPoint
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

private val PAGE_GAP = 12.dp
private const val HANDLE_DRAW_DP = 20

/**
 * The continuous reader: every page of the document is a slot in one scrolling
 * list, rasterized only while it is near the viewport.
 *
 * Rows contain one page or a two-page spread. Slots are laid out from the page's
 * media box *before* anything is rendered, so the list never resizes under the
 * user's thumb when a render lands. And [onPositionChanged] is the only thing
 * that triggers rendering, which is what keeps the resident bitmap set
 * bounded — see [CACHE_PAGES].
 */
@Composable
internal fun PageList(
    state: ViewerState,
    onPositionChanged: (ReaderPosition) -> Unit,
    onScrollTargetConsumed: () -> Unit,
    onAnnotationRevealConsumed: () -> Unit,
    onAnnotationGesture: (Int, AnnotationPoint, AnnotationPoint, List<AnnotationPoint>, Double) -> Unit,
    textSelection: TextSelectionGestures,
    contentEdit: ContentEditActions,
    onFormFieldTap: (Int, AnnotationPoint) -> Unit,
    onPinch: (zoomFactor: Double) -> Unit,
    onFileDrop: (Int, AnnotationPoint, DragAndDropEvent) -> Boolean,
    modifier: Modifier = Modifier,
    columns: Int = 1,
) {
    val listState = rememberLazyListState()
    val rows = remember(state.pageCount, columns) { PageRows(state.pageCount, columns) }
    var preferredPage by remember(state.documentId) { mutableStateOf(state.pageIndex) }
    val horizontalScrollState = rememberScrollState()
    val pinch = remember { PinchState() }
    // The pinch detector outlives recompositions, so it reads these rather than the parameters.
    val laidOutZoom by rememberUpdatedState(state.zoomFactor)
    val currentOnPinch by rememberUpdatedState(onPinch)
    // Where the list stood when the fingers lifted, to scroll back under them once the new zoom is laid out.
    var pinchAnchor by remember { mutableStateOf<PinchAnchor?>(null) }
    val stampBitmaps = rememberStampBitmaps(state.stampImages)

    BoxWithConstraints(modifier = modifier) {
        val viewportWidthPx = with(LocalDensity.current) { maxWidth.roundToPx() }
        val viewportHeightPx = with(LocalDensity.current) { maxHeight.roundToPx() }
        val slotWidth = (maxWidth - PAGE_GAP * (columns - 1)) / columns
        val slotWidthPx = with(LocalDensity.current) { slotWidth.roundToPx() }
        val pageWidth = slotWidth * state.zoomFactor.toFloat()
        val rowWidth = pageWidth * columns + PAGE_GAP * (columns - 1)
        val pageStepPx = with(LocalDensity.current) { (pageWidth + PAGE_GAP).roundToPx() }

        LaunchedEffect(columns, state.documentId) {
            preferredPage = state.pageIndex
            pinchAnchor = null
            pinch.target = null
            horizontalScrollState.scrollTo(0)
            if (rows.count > 0) listState.scrollToItem(rows.row(preferredPage))
        }

        LaunchedEffect(listState, slotWidthPx, viewportHeightPx, state.zoomFactor, rows) {
            snapshotFlow { listState.layoutInfo to preferredPage }
                .map { (info, preferred) ->
                    val visible = info.visibleItemsInfo
                    // Null exactly when nothing is laid out yet, which is what
                    // makes first()/last() below safe.
                    val current = dominantPage(
                        visible.map { item -> VisiblePage(item.index, item.offset, item.size) },
                        info.viewportStartOffset,
                        info.viewportEndOffset,
                    )
                    current?.let {
                        ReaderPosition(
                            first = rows.first(visible.first().index),
                            last = rows.last(visible.last().index),
                            current = rows.current(it, preferred),
                            viewportWidthPx = slotWidthPx,
                            zoomFactor = state.zoomFactor,
                            viewportHeightPx = viewportHeightPx,
                        )
                    }
                }
                .distinctUntilChanged()
                .collect { position -> if (position != null) onPositionChanged(position) }
        }

        LaunchedEffect(state.scrollTarget) {
            val target = state.scrollTarget ?: return@LaunchedEffect
            preferredPage = target
            listState.animateScrollToItem(rows.row(target))
            if (columns == 2) {
                val offset = pageStepPx * (target % columns)
                horizontalScrollState.animateScrollTo(offset)
            }
            onScrollTargetConsumed()
        }

        val pageWidthPx = with(LocalDensity.current) { pageWidth.roundToPx() }
        LaunchedEffect(state.annotationReveal) {
            val reveal = state.annotationReveal ?: return@LaunchedEffect
            preferredPage = reveal.pageIndex
            val viewport = RevealViewport(pageWidthPx, viewportWidthPx, viewportHeightPx)
            val offset = pageStepPx * (reveal.pageIndex % columns)
            revealAnnotation(reveal, state.pageSizes.getOrNull(reveal.pageIndex), viewport, listState, horizontalScrollState, rows.row(reveal.pageIndex), offset)
            onAnnotationRevealConsumed()
        }

        LaunchedEffect(state.zoomFactor) {
            val anchor = pinchAnchor ?: return@LaunchedEffect
            pinchAnchor = null
            // The list only knows the new page sizes once it has laid them out.
            withFrameNanos { }
            val ratio = state.zoomFactor / anchor.fromZoom
            listState.scrollToItem(anchor.firstItem, anchoredScroll(anchor.firstItemOffset, anchor.focus.y, ratio))
            horizontalScrollState.scrollTo(anchoredScroll(anchor.scrollX, anchor.focus.x, ratio))
            pinch.target = null
        }

        // Keep one horizontal position for the entire page column. Geometry,
        // rather than bitmap scaling, changes with zoom so each page re-renders
        // sharply; a pinch scales the picture only until its zoom is laid out.
        Box(
            modifier = Modifier
                .fillMaxSize()
                .pinchToZoom(enabled = state.pageCount > 0, laidOutZoom = { laidOutZoom }, pinch = pinch) { target, focus ->
                    if (target == laidOutZoom) {
                        pinch.target = null
                    } else {
                        pinchAnchor = PinchAnchor(
                            fromZoom = laidOutZoom,
                            firstItem = listState.firstVisibleItemIndex,
                            firstItemOffset = listState.firstVisibleItemScrollOffset,
                            scrollX = horizontalScrollState.value,
                            focus = focus,
                        )
                        currentOnPinch(target)
                    }
                }
                .horizontalScroll(horizontalScrollState),
        ) {
            LazyColumn(
                state = listState,
                modifier = Modifier.width(rowWidth).fillMaxHeight(),
                verticalArrangement = Arrangement.spacedBy(PAGE_GAP),
            ) {
                items(count = rows.count, key = { index -> rows.first(index) }) { row ->
                    Row(horizontalArrangement = Arrangement.spacedBy(PAGE_GAP), verticalAlignment = Alignment.Top) {
                        for (index in rows.first(row)..rows.last(row)) {
                            Box(Modifier.width(pageWidth)) {
                                PageSlot(
                                    pageNumber = index + 1,
                                    page = state.pages[index],
                                    bridge = state.bridgePages[index],
                                    size = state.pageSizes.getOrNull(index),
                                    state = state,
                                    stampBitmaps = stampBitmaps,
                                    onAnnotationGesture = onAnnotationGesture,
                                    textSelection = textSelection,
                                    contentEdit = contentEdit,
                                    onFormFieldTap = onFormFieldTap,
                                    onFileDrop = onFileDrop,
                                )
                            }
                        }
                        if (rows.last(row) - rows.first(row) + 1 < columns) Spacer(Modifier.width(pageWidth))
                    }
                }
            }
        }
    }
}

@Composable
private fun PageSlot(
    pageNumber: Int,
    page: ImageBitmap?,
    bridge: ImageBitmap?,
    size: PageSize?,
    state: ViewerState,
    stampBitmaps: Map<Long, ImageBitmap>,
    onAnnotationGesture: (Int, AnnotationPoint, AnnotationPoint, List<AnnotationPoint>, Double) -> Unit,
    textSelection: TextSelectionGestures,
    contentEdit: ContentEditActions,
    onFormFieldTap: (Int, AnnotationPoint) -> Unit,
    onFileDrop: (Int, AnnotationPoint, DragAndDropEvent) -> Boolean,
) {
    val haptics = LocalHapticFeedback.current
    var origin by remember { mutableStateOf<AnnotationPoint?>(null) }
    var current by remember { mutableStateOf<AnnotationPoint?>(null) }
    var stroke by remember { mutableStateOf(emptyList<AnnotationPoint>()) }
    var pageWidthPx by remember { mutableStateOf(0) }
    var pageCoordinates by remember { mutableStateOf<LayoutCoordinates?>(null) }
    var moveDrag by remember { mutableStateOf<MoveDrag?>(null) }
        val density = LocalDensity.current.density.toDouble()
        val pageIndex = pageNumber - 1
        // Edit content claims every tap on the page, and nothing else: no drag-select, no annotation drag —
        // except dragging the box of an armed move on this page.
        val contentMode = state.contentEdit != null
        val movingHere = state.contentEdit?.moving?.takeIf { it.pageIndex == pageIndex }
        // So does a field placement or move the Form fields panel armed.
        val formMode = state.formFields?.armed != null
        val tapMode = contentMode || formMode
        LaunchedEffect(pageIndex, contentMode) { if (contentMode) contentEdit.onPageShown(pageIndex) }
        Box(
        modifier = Modifier
            .fillMaxWidth()
            .aspectRatio(size.aspectRatio())
            .clip(RoundedCornerShape(2.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant),
        contentAlignment = Alignment.Center,
    ) {
        if (bridge != null) {
            Image(
                bitmap = bridge,
                contentDescription = null,
                modifier = Modifier.fillMaxSize(),
                contentScale = ContentScale.Fit,
            )
        }
        if (page != null) {
            Image(
                bitmap = page,
                contentDescription = "Page $pageNumber",
                modifier = Modifier.fillMaxSize(),
                contentScale = ContentScale.Fit,
            )
        }
        if (page == null && bridge == null) {
            CircularProgressIndicator(modifier = Modifier.size(24.dp))
        }
        if (size != null) {
            val scale = maxOf(1, pageWidthPx).toDouble() / size.widthPt
            // Taps and overlays both go through the page's turn: on a rotated page a bare y-flip misplaces every one.
            val placement = PagePlacement(size, scale)
            fun point(offset: Offset) = placement.pointToPdf(offset.x.toDouble(), offset.y.toDouble())
            androidx.compose.foundation.Canvas(
                modifier = Modifier
                    .fillMaxSize()
                    .onSizeChanged { pageWidthPx = it.width }
                    .onGloballyPositioned { pageCoordinates = it }
                    // A file dropped on this page: an image stamps at the drop point, through the same turn as a tap.
                    .fileDropTarget { event ->
                        val coordinates = pageCoordinates ?: return@fileDropTarget false
                        val at = dropPosition(event, coordinates)
                        onFileDrop(pageIndex, point(at), event)
                    }
                    .pointerInput(pageNumber, placement, state.activeAnnotationTool, state.selectedAnnotationId, contentMode, formMode) {
                        detectTapGestures(
                            // In pointer mode a long-press belongs to text
                            // selection below. Declaring it here is what stops
                            // the finger lifting from also counting as a tap,
                            // which would clear the selection it just made.
                            onLongPress = if (state.activeAnnotationTool == AnnotationTool.Pointer && !tapMode) ({ }) else null,
                        ) { offset ->
                            val tap = point(offset)
                            val reach = handleReachPoints(HANDLE_REACH_DP, density, scale)
                            when {
                                formMode -> onFormFieldTap(pageIndex, tap)
                                contentMode -> contentEdit.onTap(pageIndex, tap, reach)
                                else -> onAnnotationGesture(pageIndex, tap, tap, emptyList(), reach)
                            }
                        }
                    }
                    .pointerInput(pageNumber, placement, movingHere) {
                        // An armed move: grabbing the run's or image's box drags it, its pixels following
                        // the finger, and lifting places it — the same edit a tap at the new corner sends.
                        // A drag anywhere else stays the list's scroll.
                        val target = movingHere ?: return@pointerInput
                        val reach = handleReachPoints(HANDLE_REACH_DP, density, scale)
                        awaitEachGesture {
                            val down = awaitFirstDown(requireUnconsumed = false)
                            val from = point(down.position)
                            if (!grabs(target.bounds, from, reach)) return@awaitEachGesture
                            val slopChange = awaitTouchSlopOrCancellation(down.id) { change, _ -> change.consume() } ?: return@awaitEachGesture
                            moveDrag = MoveDrag(from, point(slopChange.position))
                            val completed = drag(slopChange.id) { change ->
                                change.consume()
                                moveDrag = MoveDrag(from, point(change.position))
                            }
                            val end = moveDrag?.to
                            moveDrag = null
                            if (completed && end != null) contentEdit.onTap(pageIndex, draggedCorner(target.bounds, from, end), reach)
                        }
                    }
                    .pointerInput(pageNumber, placement, state.activeAnnotationTool, tapMode) {
                        // Long-press, then drag: the Android text-selection
                        // gesture. A plain drag stays the list's scroll — it
                        // moves past touch slop before the long-press fires,
                        // which cancels this detector.
                        if (state.activeAnnotationTool != AnnotationTool.Pointer || tapMode) return@pointerInput
                        detectDragGesturesAfterLongPress(
                            onDragStart = { offset ->
                                haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                                textSelection.onStart(pageIndex, point(offset))
                            },
                            onDrag = { change, _ ->
                                change.consume()
                                textSelection.onMove(point(change.position))
                            },
                            onDragEnd = textSelection.onEnd,
                            onDragCancel = textSelection.onEnd,
                        )
                    }
                    .pointerInput(pageNumber, placement, state.activeAnnotationTool, state.selectedAnnotationId, tapMode) {
                        if (tapMode) return@pointerInput
                        val reach = handleReachPoints(HANDLE_REACH_DP, density, scale)
                        awaitEachGesture {
                            val down = awaitFirstDown(requireUnconsumed = false)
                            val downPoint = point(down.position)
                            val selected = state.annotations.firstOrNull { it.id == state.selectedAnnotationId && it.pageIndex == pageIndex }
                            // A drawing tool always claims the gesture; the pointer
                            // tool only claims it on a hit against the selected
                            // annotation's body or a resize handle. Anything else
                            // in pointer mode is left unconsumed so the enclosing
                            // LazyColumn can still claim it as a scroll.
                            val claimsGesture = state.activeAnnotationTool != AnnotationTool.Pointer ||
                                (selected != null && dragModeAt(selected, downPoint, reach) != null)
                            if (!claimsGesture) return@awaitEachGesture

                            val slopChange = awaitTouchSlopOrCancellation(down.id) { change, _ -> change.consume() }
                            if (slopChange != null) {
                                origin = point(slopChange.position)
                                current = origin
                                stroke = listOf(requireNotNull(origin))
                                val completed = drag(slopChange.id) { change ->
                                    change.consume()
                                    current = point(change.position)
                                    if (state.activeAnnotationTool == AnnotationTool.Ink) stroke = stroke + requireNotNull(current)
                                }
                                val start = origin
                                val end = current
                                if (completed && start != null && end != null) onAnnotationGesture(pageIndex, start, end, stroke, reach)
                                origin = null; current = null; stroke = emptyList()
                            }
                        }
                    },
            ) {
                // While an armed move is dragged, where its box would land now.
                val dragged = movingHere?.let { target -> moveDrag?.let { drag -> movedRect(target.bounds, draggedCorner(target.bounds, drag.from, drag.to)) } }
                // Only the renderer paints the words or the picture, so the drag lifts them off the page's own bitmap.
                if (movingHere != null && dragged != null) (page ?: bridge)?.let { bitmap -> drawLifted(bitmap, placement.placeRect(movingHere.bounds), placement.placeRect(dragged)) }
                // Images first, so a caption's outline sits on top of the photo it is printed over.
                // The run or image an armed move will place is drawn heavier, so the reader sees what the next tap moves.
                state.contentEdit?.images?.get(pageIndex)?.forEach { image ->
                    val moving = state.contentEdit.movingImage?.id == image.id
                    val bounds = if (moving && dragged != null) dragged else image.bounds
                    val placed = placement.placeRect(bounds)
                    drawRect(
                        if (moving) Color(0xFF20A060) else Color(0x9920A060),
                        placed.topLeft,
                        placed.size,
                        style = Stroke(if (moving) 5f else 2f),
                    )
                }
                // Solid where the run keeps its font, dashed where a retype swaps in a standard one:
                // otherwise nothing tells the two apart before the reader has typed.
                state.contentEdit?.runs?.get(pageIndex)?.forEach { run ->
                    val moving = state.contentEdit.movingText?.id == run.id
                    val bounds = if (moving && dragged != null) dragged else run.bounds
                    val placed = placement.placeRect(bounds)
                    drawRect(
                        if (run.substitutesFont) Color(if (moving) 0xFFAA5ADC else 0x99AA5ADC) else Color(if (moving) 0xFF2878EB else 0x992878EB),
                        placed.topLeft,
                        placed.size,
                        style = Stroke(if (moving) 5f else 2f, pathEffect = if (run.substitutesFont) PathEffect.dashPathEffect(floatArrayOf(6f, 4f)) else null),
                    )
                }
                state.textSelection?.takeIf { it.pageIndex == pageIndex }?.rects?.forEach { rect ->
                    val placed = placement.placeRect(rect)
                    drawRect(Color(0x553373E6), placed.topLeft, placed.size)
                }
                // Current search match only, mirroring the Linux/Windows shells:
                // one hit highlighted at a time, cleared as soon as the user
                // steps to another match or page.
                state.searchHits.getOrNull(state.searchIndex)?.takeIf { it.pageIndex == pageIndex }?.characterBounds?.forEach { rect ->
                    val placed = placement.placeRect(rect)
                    drawRect(Color(0x60FFD60A), placed.topLeft, placed.size)
                    drawRect(Color(0xFFFF8C00), placed.topLeft, placed.size, style = Stroke(1f))
                }
                val previewOrigin = origin
                val previewCurrent = current
                val selectedAnnotation = state.annotations.firstOrNull { it.id == state.selectedAnnotationId && it.pageIndex == pageIndex }
                // Live preview of an in-progress move/resize on the selected
                // annotation: without this, dragging it showed no feedback
                // until release actually applied the edit.
                val draggedAnnotation = if (state.activeAnnotationTool == AnnotationTool.Pointer && selectedAnnotation != null && previewOrigin != null && previewCurrent != null) {
                    when (val mode = dragModeAt(selectedAnnotation, previewOrigin, handleReachPoints(HANDLE_REACH_DP, density, scale))) {
                        DragMode.Move -> selectedAnnotation.translated(previewCurrent.x - previewOrigin.x, previewCurrent.y - previewOrigin.y)
                        is DragMode.Resize -> selectedAnnotation.rect?.let { rect -> selectedAnnotation.copy(rect = resizedRect(rect, mode.corner, previewCurrent)) }
                        null -> null
                    }
                } else null
                state.annotations.filter { it.pageIndex == pageIndex }.forEach { annotation ->
                    val shown = if (annotation.id == draggedAnnotation?.id) draggedAnnotation else annotation
                    drawAnnotationShape(shown, placement, selected = annotation.id == state.selectedAnnotationId, stampBitmaps[annotation.id])
                }
                // Live preview of the annotation being placed: without this, a
                // highlight/underline/strikeout/ink stroke was invisible until
                // the finger lifted and onAnnotationGesture actually applied it.
                if (state.activeAnnotationTool != AnnotationTool.Pointer && previewOrigin != null && previewCurrent != null) {
                    drawAnnotationShape(placementAnnotation(state.activeAnnotationTool, pageIndex, previewOrigin, previewCurrent, stroke), placement, selected = false)
                }
            }
        }
    }
}

/** An armed move's drag, from where the finger grabbed the box to where it is now, in PDF points. */
private class MoveDrag(val from: AnnotationPoint, val to: AnnotationPoint)

/**
 * Copies the part of [bitmap] under [from] — the page as drawn, so a run's own
 * glyphs or an image's own pixels — to [to]. Clipped to the page: a box that
 * hangs off it has no pixels there to lift.
 */
private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawLifted(bitmap: ImageBitmap, from: PlacedRect, to: PlacedRect) {
    val left = from.left.coerceAtLeast(0.0)
    val top = from.top.coerceAtLeast(0.0)
    val right = (from.left + from.width).coerceAtMost(size.width.toDouble())
    val bottom = (from.top + from.height).coerceAtMost(size.height.toDouble())
    if (right <= left || bottom <= top) return
    val sx = bitmap.width / size.width.toDouble()
    val sy = bitmap.height / size.height.toDouble()
    val srcX = (left * sx).roundToInt().coerceIn(0, bitmap.width - 1)
    val srcY = (top * sy).roundToInt().coerceIn(0, bitmap.height - 1)
    val srcSize = IntSize(
        ((right - left) * sx).roundToInt().coerceIn(1, bitmap.width - srcX),
        ((bottom - top) * sy).roundToInt().coerceIn(1, bitmap.height - srcY),
    )
    drawImage(
        bitmap,
        srcOffset = IntOffset(srcX, srcY),
        srcSize = srcSize,
        dstOffset = IntOffset((to.left + left - from.left).roundToInt(), (to.top + top - from.top).roundToInt()),
        dstSize = IntSize((right - left).roundToInt().coerceAtLeast(1), (bottom - top).roundToInt().coerceAtLeast(1)),
        alpha = 0.85f,
    )
}

/** The list's position, in the zoom it was taken at, when a pinch lifted. */
private class PinchAnchor(val fromZoom: Double, val firstItem: Int, val firstItemOffset: Int, val scrollX: Int, val focus: Offset)

private val PlacedRect.topLeft get() = Offset(left.toFloat(), top.toFloat())
private val PlacedRect.size get() = androidx.compose.ui.geometry.Size(width.toFloat(), height.toFloat())
private fun PagePlacement.offset(x: Double, y: Double) = placePoint(AnnotationPoint(x, y)).let { Offset(it.x.toFloat(), it.y.toFloat()) }

/**
 * Draws [annotation] where [placement] puts it. Rules and handles are placed as
 * page-space points, not as edges of the placed rect: on a turned page the
 * rect's PDF bottom edge — where an underline sits — is no longer at the bottom.
 */
private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawAnnotationShape(
    annotation: dev.vitela.pdf.core.Annotation,
    placement: PagePlacement,
    selected: Boolean,
    stamp: ImageBitmap? = null,
) {
    val color = annotation.color?.let { Color(it.red, it.green, it.blue) } ?: Color(0xFF3366CC)
    annotation.rect?.let { rect ->
        val placed = placement.placeRect(rect)
        val right = rect.x + rect.width
        val top = rect.y + rect.height
        when (annotation.kind) {
            dev.vitela.pdf.core.AnnotationKind.Highlight -> drawRect(color.copy(alpha = if (selected) .65f else .4f), placed.topLeft, placed.size)
            dev.vitela.pdf.core.AnnotationKind.Underline -> drawLine(color, placement.offset(rect.x, rect.y), placement.offset(right, rect.y), 2f)
            dev.vitela.pdf.core.AnnotationKind.Strikeout -> drawLine(color, placement.offset(rect.x, rect.y + rect.height / 2), placement.offset(right, rect.y + rect.height / 2), 2f)
            dev.vitela.pdf.core.AnnotationKind.Stamp -> {
                stamp?.let { drawStampImage(it, rect, placement) }
                // The picture alone is the stamp; the outline only marks it while selected or still undecoded.
                if (stamp == null || selected) drawRect(color, placed.topLeft, placed.size, style = Stroke(if (selected) 3f else 2f))
            }
            else -> drawRect(color, placed.topLeft, placed.size, style = Stroke(if (selected) 3f else 2f))
        }
        if (selected && annotation.supportsResize) {
            // Sized in dp, not raw pixels, so the handle is a real, consistently
            // grabbable touch target across device densities — the previous 8px
            // square was easy to miss with a finger.
            val handleSize = HANDLE_DRAW_DP.dp.toPx()
            val half = handleSize / 2f
            listOf(placement.offset(rect.x, top), placement.offset(right, top), placement.offset(rect.x, rect.y), placement.offset(right, rect.y)).forEach { handle -> drawRect(Color(0xFF1A59D9), handle - Offset(half, half), androidx.compose.ui.geometry.Size(handleSize, handleSize)) }
        }
    }
    if (annotation.kind == dev.vitela.pdf.core.AnnotationKind.Ink && annotation.points.size > 1) annotation.points.zipWithNext().forEach { (a, b) -> drawLine(color, placement.offset(a.x, a.y), placement.offset(b.x, b.y), if (selected) 3f else 2f) }
}

/** The reader's long-press drag-select callbacks, in PDF-space points. */
internal class TextSelectionGestures(
    val onStart: (pageIndex: Int, point: AnnotationPoint) -> Unit,
    val onMove: (AnnotationPoint) -> Unit,
    val onEnd: () -> Unit,
)
