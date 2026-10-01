package dev.vitela.pdf.viewer

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

/**
 * Direct navigation needs pages to go to and a reader to scroll. The Organize
 * grid replaces the reader, so a target set there would fire only after the
 * grid closes, over the page Organize itself returns to.
 */
internal fun pageNavigationEnabled(state: ViewerState): Boolean =
    state.pageCount > 0 && !state.isLoading && state.organize == null

/**
 * The numbered page list (parity with the Linux and Windows page lists):
 * picking a row scrolls the reader there. It opens on the page counter's page
 * (the one covering most of the viewport) and is never a document edit.
 */
@Composable
internal fun PageNavigationDialog(
    pageCount: Int,
    currentPage: Int,
    onSelect: (Int) -> Unit,
    onDismiss: () -> Unit,
) {
    val listState = rememberLazyListState(initialFirstVisibleItemIndex = boundedPageIndex(currentPage, pageCount))
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Go to page") },
        text = {
            LazyColumn(state = listState, modifier = Modifier.heightIn(max = 360.dp)) {
                items(pageCount) { pageIndex ->
                    val current = pageIndex == currentPage
                    Text(
                        "Page ${pageIndex + 1}",
                        fontWeight = if (current) FontWeight.Bold else null,
                        color = if (current) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier
                            .fillMaxWidth()
                            .semantics { selected = current }
                            .clickable { onSelect(pageIndex) }
                            .padding(vertical = 12.dp),
                    )
                }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
