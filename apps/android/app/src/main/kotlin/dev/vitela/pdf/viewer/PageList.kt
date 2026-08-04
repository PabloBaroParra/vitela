package dev.vitela.pdf.viewer

import androidx.compose.foundation.Image
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
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
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
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
 * Two invariants make this work on a phone. Slots are laid out from the page's
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
    onAnnotationGesture: (Int, AnnotationPoint, AnnotationPoint, List<AnnotationPoint>, Double) -> Unit,
    modifier: Modifier = Modifier,
) {
    val listState = rememberLazyListState()
    val horizontalScrollState = rememberScrollState()

    BoxWithConstraints(modifier = modifier) {
        val viewportWidthPx = with(LocalDensity.current) { maxWidth.roundToPx() }
        val pageWidth = maxWidth * state.zoomFactor.toFloat()

        LaunchedEffect(listState, viewportWidthPx, state.zoomFactor) {
            snapshotFlow { listState.layoutInfo }
                .map { info ->
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
                            first = visible.first().index,
                            last = visible.last().index,
                            current = it,
                            viewportWidthPx = viewportWidthPx,
                            zoomFactor = state.zoomFactor,
                        )
                    }
                }
                .distinctUntilChanged()
                .collect { position -> if (position != null) onPositionChanged(position) }
        }

        LaunchedEffect(state.scrollTarget) {
            val target = state.scrollTarget ?: return@LaunchedEffect
            listState.animateScrollToItem(target)
            onScrollTargetConsumed()
        }

        // Keep one horizontal position for the entire page column. Geometry,
        // rather than bitmap scaling, changes with zoom so each page re-renders sharply.
        Box(modifier = Modifier.fillMaxSize().horizontalScroll(horizontalScrollState)) {
            LazyColumn(
                state = listState,
                modifier = Modifier.width(pageWidth).fillMaxHeight(),
                verticalArrangement = Arrangement.spacedBy(PAGE_GAP),
            ) {
                items(count = state.pageCount, key = { index -> index }) { index ->
                    PageSlot(
                        pageNumber = index + 1,
                        page = state.pages[index],
                        bridge = state.bridgePages[index],
                        size = state.pageSizes.getOrNull(index),
                        state = state,
                        onAnnotationGesture = onAnnotationGesture,
                    )
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
    onAnnotationGesture: (Int, AnnotationPoint, AnnotationPoint, List<AnnotationPoint>, Double) -> Unit,
) {
    var origin by remember { mutableStateOf<AnnotationPoint?>(null) }
    var current by remember { mutableStateOf<AnnotationPoint?>(null) }
    var stroke by remember { mutableStateOf(emptyList<AnnotationPoint>()) }
    var pageWidthPx by remember { mutableStateOf(0) }
        val density = LocalDensity.current.density.toDouble()
        val pageIndex = pageNumber - 1
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
            val screenScale = scale.toFloat()
            fun point(offset: Offset) = AnnotationPoint(offset.x / scale, size.heightPt - offset.y / scale)
            androidx.compose.foundation.Canvas(
                modifier = Modifier
                    .fillMaxSize()
                    .onSizeChanged { pageWidthPx = it.width }
                    .pointerInput(pageNumber, scale, state.activeAnnotationTool, state.selectedAnnotationId) {
                        detectTapGestures { offset ->
                            val tap = point(offset)
                            onAnnotationGesture(pageIndex, tap, tap, emptyList(), handleReachPoints(HANDLE_REACH_DP, density, scale))
                        }
                    }
                    .pointerInput(pageNumber, scale, state.activeAnnotationTool, state.selectedAnnotationId) {
                        detectDragGestures(
                            onDragStart = { offset ->
                                origin = point(offset)
                                current = origin
                                stroke = listOf(requireNotNull(origin))
                            },
                            onDrag = { change, _ ->
                                change.consume()
                                current = point(change.position)
                                if (state.activeAnnotationTool == AnnotationTool.Ink) stroke = stroke + requireNotNull(current)
                            },
                            onDragEnd = {
                                val start = origin
                                val end = current
                                if (start != null && end != null) onAnnotationGesture(pageIndex, start, end, stroke, handleReachPoints(HANDLE_REACH_DP, density, scale))
                                origin = null; current = null; stroke = emptyList()
                            },
                            onDragCancel = { origin = null; current = null; stroke = emptyList() },
                        )
                    },
            ) {
                state.textSelection?.takeIf { it.pageIndex == pageIndex }?.rects?.forEach { rect ->
                    drawRect(
                        Color(0x553373E6),
                        Offset(rect.x.toFloat() * screenScale, (size.heightPt - rect.y - rect.height).toFloat() * screenScale),
                        androidx.compose.ui.geometry.Size(rect.width.toFloat() * screenScale, rect.height.toFloat() * screenScale),
                    )
                }
                // Current search match only, mirroring the Linux/Windows shells:
                // one hit highlighted at a time, cleared as soon as the user
                // steps to another match or page.
                state.searchHits.getOrNull(state.searchIndex)?.takeIf { it.pageIndex == pageIndex }?.characterBounds?.forEach { rect ->
                    val topLeft = Offset(rect.x.toFloat() * screenScale, (size.heightPt - rect.y - rect.height).toFloat() * screenScale)
                    val boundsSize = androidx.compose.ui.geometry.Size(rect.width.toFloat() * screenScale, rect.height.toFloat() * screenScale)
                    drawRect(Color(0x60FFD60A), topLeft, boundsSize)
                    drawRect(Color(0xFFFF8C00), topLeft, boundsSize, style = Stroke(1f))
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
                    drawAnnotationShape(shown, screenScale, size.heightPt, selected = annotation.id == state.selectedAnnotationId)
                }
                // Live preview of the annotation being placed: without this, a
                // highlight/underline/strikeout/ink stroke was invisible until
                // the finger lifted and onAnnotationGesture actually applied it.
                if (state.activeAnnotationTool != AnnotationTool.Pointer && previewOrigin != null && previewCurrent != null) {
                    drawAnnotationShape(placementAnnotation(state.activeAnnotationTool, pageIndex, previewOrigin, previewCurrent, stroke), screenScale, size.heightPt, selected = false)
                }
            }
        }
    }
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawAnnotationShape(
    annotation: dev.vitela.pdf.core.Annotation,
    screenScale: Float,
    pageHeightPt: Double,
    selected: Boolean,
) {
    val color = annotation.color?.let { Color(it.red, it.green, it.blue) } ?: Color(0xFF3366CC)
    annotation.rect?.let { rect ->
        val top = (pageHeightPt - rect.y - rect.height).toFloat() * screenScale
        val size = androidx.compose.ui.geometry.Size(rect.width.toFloat() * screenScale, rect.height.toFloat() * screenScale)
        when (annotation.kind) {
            dev.vitela.pdf.core.AnnotationKind.Highlight -> drawRect(color.copy(alpha = if (selected) .65f else .4f), Offset(rect.x.toFloat() * screenScale, top), size)
            dev.vitela.pdf.core.AnnotationKind.Underline -> drawLine(color, Offset(rect.x.toFloat() * screenScale, top + size.height), Offset((rect.x + rect.width).toFloat() * screenScale, top + size.height), 2f)
            dev.vitela.pdf.core.AnnotationKind.Strikeout -> drawLine(color, Offset(rect.x.toFloat() * screenScale, top + size.height / 2), Offset((rect.x + rect.width).toFloat() * screenScale, top + size.height / 2), 2f)
            else -> drawRect(color, Offset(rect.x.toFloat() * screenScale, top), size, style = Stroke(if (selected) 3f else 2f))
        }
        if (selected && annotation.supportsResize) {
            // Sized in dp, not raw pixels, so the handle is a real, consistently
            // grabbable touch target across device densities — the previous 8px
            // square was easy to miss with a finger.
            val handleSize = HANDLE_DRAW_DP.dp.toPx()
            val half = handleSize / 2f
            listOf(Offset(rect.x.toFloat() * screenScale, top), Offset((rect.x + rect.width).toFloat() * screenScale, top), Offset(rect.x.toFloat() * screenScale, top + size.height), Offset((rect.x + rect.width).toFloat() * screenScale, top + size.height)).forEach { handle -> drawRect(Color(0xFF1A59D9), handle - Offset(half, half), androidx.compose.ui.geometry.Size(handleSize, handleSize)) }
        }
    }
    if (annotation.kind == dev.vitela.pdf.core.AnnotationKind.Ink && annotation.points.size > 1) annotation.points.zipWithNext().forEach { (a, b) -> drawLine(color, Offset(a.x.toFloat() * screenScale, (pageHeightPt - a.y).toFloat() * screenScale), Offset(b.x.toFloat() * screenScale, (pageHeightPt - b.y).toFloat() * screenScale), if (selected) 3f else 2f) }
}
