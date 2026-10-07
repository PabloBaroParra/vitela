package dev.vitela.pdf.home

import android.graphics.BitmapFactory
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.time.ZoneId

/**
 * What Home's Recent section can do with a card. [thumbnail] reads a card's
 * first-page PNG; the section decodes it off the main thread.
 */
internal class RecentActions(
    val onOpen: (RecentDocument) -> Unit,
    val onRemove: (RecentDocument) -> Unit,
    val thumbnail: suspend (String) -> ByteArray?,
)

/**
 * The documents opened here last, grouped Today / Yesterday / Earlier like
 * the desktop Home. A long-press (or the accessibility action) offers
 * **Remove from Recent**, which takes the card off for good — it comes back
 * only when the document is opened again.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun HomeRecents(entries: List<RecentDocument>, enabled: Boolean, actions: RecentActions) {
    // Re-read with the list: a card opened at 23:59 moves to Yesterday on the next change, not mid-frame.
    val now = remember(entries) { System.currentTimeMillis() }
    val zone = remember(entries) { ZoneId.systemDefault() }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text("Recent", style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold)
        if (entries.isEmpty()) {
            Text(
                "No recent documents yet. Open a PDF and it will show up here.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        groupedByDay(entries, now, zone).forEach { (day, documents) ->
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Surface(shape = MaterialTheme.shapes.small, color = MaterialTheme.colorScheme.surfaceVariant) {
                    Text(day.chip, style = MaterialTheme.typography.labelMedium, modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp))
                }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    documents.forEach { entry -> key(entry.uri) { RecentCard(entry, openedText(entry, now, zone), enabled, actions) } }
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun RecentCard(entry: RecentDocument, opened: String, enabled: Boolean, actions: RecentActions) {
    var menu by remember { mutableStateOf(false) }
    // Keyed by when it was opened too: reopening replaces the preview under the same URI.
    val preview by produceState<ImageBitmap?>(null, entry.uri, entry.openedAt) {
        value = actions.thumbnail(entry.uri)?.let { png ->
            withContext(Dispatchers.Default) { BitmapFactory.decodeByteArray(png, 0, png.size)?.asImageBitmap() }
        }
    }
    val shape = MaterialTheme.shapes.medium
    Box {
        Surface(
            shape = shape,
            color = MaterialTheme.colorScheme.surface,
            border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
            modifier = Modifier
                .width(124.dp)
                .clip(shape)
                .combinedClickable(
                    enabled = enabled,
                    onClickLabel = "Open",
                    onLongClickLabel = "Show options",
                    onClick = { actions.onOpen(entry) },
                    onLongClick = { menu = true },
                )
                .semantics {
                    customActions = listOf(CustomAccessibilityAction("Remove from Recent") { actions.onRemove(entry); true })
                },
        ) {
            Column(Modifier.padding(8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Box(
                    Modifier.size(width = 108.dp, height = 140.dp).clip(MaterialTheme.shapes.small).background(MaterialTheme.colorScheme.surfaceVariant),
                    contentAlignment = Alignment.Center,
                ) {
                    val bitmap = preview
                    if (bitmap != null) {
                        Image(bitmap, contentDescription = null, contentScale = ContentScale.Fit, modifier = Modifier.fillMaxSize())
                    } else {
                        // Encrypted documents never get a preview; neither does one whose render failed.
                        Icon(painterResource(R.drawable.ic_shell_files), contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.size(28.dp))
                    }
                }
                Text(entry.displayName, style = MaterialTheme.typography.bodySmall, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(opened, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2)
            }
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(
                text = { Text("Remove from Recent") },
                leadingIcon = { Icon(painterResource(R.drawable.ic_shell_delete), contentDescription = null, modifier = Modifier.size(18.dp)) },
                onClick = {
                    menu = false
                    actions.onRemove(entry)
                },
            )
        }
    }
}
