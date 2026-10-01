package dev.vitela.pdf.viewer

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.core.DocumentBlock

/**
 * Organize > Documents: one card per run of pages from the same PDF, covered by
 * its first page's thumbnail — the same position-keyed picture the Pages view
 * draws, so switching views renders nothing twice.
 */
@Composable
internal fun OrganizeDocumentList(state: ViewerState, organize: OrganizeState, actions: OrganizeActions, modifier: Modifier = Modifier) {
    LazyColumn(
        contentPadding = PaddingValues(4.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        modifier = modifier.fillMaxSize(),
    ) {
        // Keyed by position *and* run: a move can merge two blocks or split one,
        // so no card keeps its identity across an edit anyway.
        itemsIndexed(organize.blocks, key = { _, block -> "${block.start}:${block.count}" }) { position, block ->
            DocumentCard(position, block, state, organize, actions)
        }
    }
}

@Composable
private fun DocumentCard(position: Int, block: DocumentBlock, state: ViewerState, organize: OrganizeState, actions: OrganizeActions) {
    val cover = organize.thumbnails[block.start]
    LaunchedEffect(block.start, cover == null, organize.version) {
        if (cover == null) actions.onThumbnail(block.start)
    }
    val title = blockTitle(block, state.title, state.importedSourceNames)
    val idle = !organize.busy
    val last = organize.blocks.lastIndex
    Row(
        modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(4.dp)).background(MaterialTheme.colorScheme.surface).padding(6.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .width(72.dp)
                .aspectRatio(state.pageSizes.getOrNull(block.start)?.aspectRatio() ?: DEFAULT_PAGE_ASPECT_RATIO)
                .background(MaterialTheme.colorScheme.surfaceVariant),
            contentAlignment = Alignment.Center,
        ) {
            if (cover != null) {
                Image(bitmap = cover, contentDescription = null, modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
            } else {
                CircularProgressIndicator(modifier = Modifier.size(20.dp))
            }
        }
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(blockMeta(block), style = MaterialTheme.typography.bodySmall)
            // Moves first, the delete last: the destructive control keeps the
            // end of the row, as on the page cards and the Linux shell's.
            Row(verticalAlignment = Alignment.CenterVertically) {
                CardButton("▲", "Move $title up", idle && position > 0) { actions.onMoveBlock(block, -1) }
                CardButton("▼", "Move $title down", idle && position < last) { actions.onMoveBlock(block, 1) }
                CardButton("↺", "Rotate $title left", idle) { actions.onRotateBlock(block, -90) }
                CardButton("↻", "Rotate $title right", idle) { actions.onRotateBlock(block, 90) }
                CardButton("✕", "Delete $title", idle && block.count < state.pageCount) { actions.onDeleteBlock(block) }
            }
        }
    }
}
