package dev.vitela.pdf.viewer

import androidx.compose.foundation.Image
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.core.PageSize
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

private val PAGE_GAP = 12.dp

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
                    )
                }
            }
        }
    }
}

@Composable
private fun PageSlot(pageNumber: Int, page: ImageBitmap?, bridge: ImageBitmap?, size: PageSize?) {
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
    }
}
