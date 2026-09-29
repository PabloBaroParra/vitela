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
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

/** What the Organize grid can ask of the ViewModel; one value so the screen's parameter list stays short. */
internal class OrganizeActions(
    val onToggle: () -> Unit,
    val onMove: (index: Int, delta: Int) -> Unit,
    val onRotate: (index: Int, delta: Int) -> Unit,
    val onDelete: (index: Int) -> Unit,
    val onInsertBlank: (index: Int, landscape: Boolean) -> Unit,
    val onThumbnail: (index: Int) -> Unit,
)

/**
 * Organize pages: one card per page, standing in for the reader. A page moves a
 * step at a time with the arrow buttons rather than by dragging: a drag inside
 * a scrolling grid competes with the scroll itself, and each button is a
 * labelled target a screen reader can reach. A blank page goes in before any
 * card, or after the last one from the trailing "add" card.
 */
@Composable
internal fun OrganizeGrid(state: ViewerState, organize: OrganizeState, actions: OrganizeActions, modifier: Modifier = Modifier) {
    LazyVerticalGrid(
        columns = GridCells.Adaptive(minSize = 150.dp),
        contentPadding = PaddingValues(4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        modifier = modifier.fillMaxSize(),
    ) {
        items((0 until state.pageCount).toList(), key = { it }) { index ->
            OrganizeCard(index, state.pageCount, state.pageSizes.getOrNull(index)?.aspectRatio() ?: DEFAULT_PAGE_ASPECT_RATIO, organize, actions)
        }
        // Keyed apart from the page positions, which are plain ints.
        item(key = "append") {
            Column(
                modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(4.dp)).background(MaterialTheme.colorScheme.surface).padding(6.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                InsertBlankButton(state.pageCount, "Add a blank page at the end", !organize.busy, actions.onInsertBlank)
                Text("Add page", style = MaterialTheme.typography.labelLarge)
            }
        }
    }
}

@Composable
private fun OrganizeCard(index: Int, pageCount: Int, aspectRatio: Float, organize: OrganizeState, actions: OrganizeActions) {
    val thumbnail = organize.thumbnails[index]
    // Keyed on the missing picture and the layout version: a turned page's card
    // asks again, and a thumbnail that was dropped for another layout does too.
    LaunchedEffect(index, thumbnail == null, organize.version) {
        if (thumbnail == null) actions.onThumbnail(index)
    }
    val number = index + 1
    val idle = !organize.busy
    Column(
        modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(4.dp)).background(MaterialTheme.colorScheme.surface).padding(6.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Box(
            modifier = Modifier.fillMaxWidth().aspectRatio(aspectRatio).background(MaterialTheme.colorScheme.surfaceVariant),
            contentAlignment = Alignment.Center,
        ) {
            if (thumbnail != null) {
                Image(bitmap = thumbnail, contentDescription = "Page $number", modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
            } else {
                CircularProgressIndicator(modifier = Modifier.size(20.dp))
            }
        }
        Text("$number", style = MaterialTheme.typography.labelLarge)
        Row(verticalAlignment = Alignment.CenterVertically) {
            CardButton("↺", "Rotate page $number left", idle) { actions.onRotate(index, -90) }
            CardButton("↻", "Rotate page $number right", idle) { actions.onRotate(index, 90) }
            CardButton("✕", "Delete page $number", idle && pageCount > 1) { actions.onDelete(index) }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            CardButton("◀", "Move page $number earlier", idle && index > 0) { actions.onMove(index, -1) }
            CardButton("▶", "Move page $number later", idle && index < pageCount - 1) { actions.onMove(index, 1) }
            InsertBlankButton(index, "Insert a blank page before page $number", idle, actions.onInsertBlank)
        }
    }
}

/** "+" with a choice of A4 portrait or landscape, both Windows' offer; the page goes in at [index]. */
@Composable
private fun InsertBlankButton(index: Int, description: String, enabled: Boolean, onInsert: (index: Int, landscape: Boolean) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        CardButton("+", description, enabled) { expanded = true }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            DropdownMenuItem(text = { Text("Portrait A4") }, onClick = { expanded = false; onInsert(index, false) })
            DropdownMenuItem(text = { Text("Landscape A4") }, onClick = { expanded = false; onInsert(index, true) })
        }
    }
}

@Composable
private fun CardButton(glyph: String, description: String, enabled: Boolean, onClick: () -> Unit) {
    IconButton(onClick = onClick, enabled = enabled, modifier = Modifier.size(44.dp).semantics { contentDescription = description }) {
        Text(glyph)
    }
}
