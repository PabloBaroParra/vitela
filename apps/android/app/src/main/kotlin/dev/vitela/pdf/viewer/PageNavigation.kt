package dev.vitela.pdf.viewer

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp

/**
 * Direct navigation needs pages to go to and a reader to scroll. The Organize
 * grid replaces the reader, so a target set there would fire only after the
 * grid closes, over the page Organize itself returns to.
 */
internal fun pageNavigationEnabled(state: ViewerState): Boolean =
    state.pageCount > 0 && !state.isLoading && state.organize == null

/**
 * A typed page number as a page index, or null unless it names a page of the
 * document. Unlike [boundedPageIndex] it does not clamp: "9" in a five-page
 * document is a typo, not a request for the last page (parity with Windows,
 * whose Go button stays disabled outside 1..N).
 */
internal fun typedPageIndex(text: String, pageCount: Int): Int? {
    val number = text.trim().toIntOrNull() ?: return null
    return if (number in 1..pageCount) number - 1 else null
}

/**
 * Where First page jumps, or null when the reader is already there (parity
 * with Windows, whose First button is disabled on the first page). A stale
 * [currentPage] is bounded first: the counter can lag a page removal.
 */
internal fun firstPageTarget(currentPage: Int, pageCount: Int): Int? =
    0.takeIf { pageCount > 0 && boundedPageIndex(currentPage, pageCount) > 0 }

/** Where Last page jumps, or null when the reader is already there. */
internal fun lastPageTarget(currentPage: Int, pageCount: Int): Int? =
    (pageCount - 1).takeIf { pageCount > 0 && boundedPageIndex(currentPage, pageCount) < pageCount - 1 }

/**
 * Where Previous page steps, or null on the first page: it stops there rather
 * than wrap to the end (parity with Windows). A stale [currentPage] is bounded
 * first, as for [firstPageTarget].
 */
internal fun previousPageTarget(currentPage: Int, pageCount: Int): Int? =
    (boundedPageIndex(currentPage, pageCount) - 1).takeIf { pageCount > 0 && it >= 0 }

/** Where Next page steps, or null on the last page. */
internal fun nextPageTarget(currentPage: Int, pageCount: Int): Int? =
    (boundedPageIndex(currentPage, pageCount) + 1).takeIf { pageCount > 0 && it < pageCount }

/**
 * The numbered page list (parity with the Linux and Windows page lists):
 * picking a row scrolls the reader there. Above it, a page number can be typed
 * (parity with Windows Go to page), prefilled and selected with the current
 * page so typing replaces it, and First page / Last page jump to either end.
 * It opens on the page counter's page (the one covering most of the viewport)
 * and is never a document edit.
 */
@Composable
internal fun PageNavigationDialog(
    pageCount: Int,
    currentPage: Int,
    onSelect: (Int) -> Unit,
    onDismiss: () -> Unit,
) {
    val listState = rememberLazyListState(initialFirstVisibleItemIndex = boundedPageIndex(currentPage, pageCount))
    var number by remember {
        val prefill = (boundedPageIndex(currentPage, pageCount) + 1).toString()
        mutableStateOf(TextFieldValue(prefill, selection = TextRange(0, prefill.length)))
    }
    // Validated against the live page count, not the one the dialog opened with.
    val typed = typedPageIndex(number.text, pageCount)
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Go to page") },
        text = {
            Column {
                OutlinedTextField(
                    value = number,
                    onValueChange = { number = it },
                    label = { Text("Page number (1–$pageCount)") },
                    isError = number.text.isNotBlank() && typed == null,
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Go),
                    keyboardActions = KeyboardActions(onGo = { typed?.let(onSelect) }),
                    modifier = Modifier.fillMaxWidth().focusRequester(focus),
                )
                val first = firstPageTarget(currentPage, pageCount)
                val last = lastPageTarget(currentPage, pageCount)
                Row(modifier = Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                    TextButton(onClick = { first?.let(onSelect) }, enabled = first != null) { Text("First page") }
                    TextButton(onClick = { last?.let(onSelect) }, enabled = last != null) { Text("Last page") }
                }
                LazyColumn(state = listState, modifier = Modifier.heightIn(max = 360.dp).padding(top = 8.dp)) {
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
            }
        },
        confirmButton = { TextButton(onClick = { typed?.let(onSelect) }, enabled = typed != null) { Text("Go") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
