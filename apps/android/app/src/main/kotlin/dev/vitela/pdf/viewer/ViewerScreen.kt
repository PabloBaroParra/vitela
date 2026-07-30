package dev.vitela.pdf.viewer

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.sample.SampleDocument

/**
 * Reader chrome around the continuous [PageList]. The controls are a fixed
 * header and the list takes the remaining height — the whole screen must not
 * scroll, because a lazy list inside a scrolling parent is measured with an
 * infinite height and would rasterize every page at once.
 */
@Composable
internal fun ViewerScreen(
    state: ViewerState,
    onOpen: () -> Unit,
    onOpenSample: (assetName: String, displayName: String) -> Unit,
    onPrevious: () -> Unit,
    onNext: () -> Unit,
    onZoomOut: () -> Unit,
    onZoomIn: () -> Unit,
    onSearch: (String) -> Unit,
    onPreviousMatch: () -> Unit,
    onNextMatch: () -> Unit,
    onPassword: (String) -> Unit,
    onPasswordCancel: () -> Unit,
    onPrint: () -> Unit,
    onPositionChanged: (ReaderPosition) -> Unit,
    onScrollTargetConsumed: () -> Unit,
) {
    var query by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var sampleMenuExpanded by remember { mutableStateOf(false) }
    val zoomPercentage = (state.zoomFactor * 100).toInt()
    Column(
        modifier = Modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(state.title, style = MaterialTheme.typography.headlineSmall)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = onOpen, enabled = state.canOpen) { Text("Open PDF") }
            Box {
                Button(onClick = { sampleMenuExpanded = true }, enabled = state.canOpen) { Text("Open sample") }
                DropdownMenu(expanded = sampleMenuExpanded, onDismissRequest = { sampleMenuExpanded = false }) {
                    DropdownMenuItem(
                        text = { Text("Vitela sample") },
                        onClick = {
                            sampleMenuExpanded = false
                            onOpenSample(SampleDocument.ASSET_NAME, SampleDocument.DISPLAY_NAME)
                        },
                    )
                    DropdownMenuItem(
                        text = { Text("AES-128 sample (user-aes-pass)") },
                        onClick = {
                            sampleMenuExpanded = false
                            onOpenSample(SampleDocument.AES128_ASSET_NAME, SampleDocument.AES128_DISPLAY_NAME)
                        },
                    )
                    DropdownMenuItem(
                        text = { Text("RC4-128 sample (user-rc4-pass)") },
                        onClick = {
                            sampleMenuExpanded = false
                            onOpenSample(SampleDocument.RC4_128_ASSET_NAME, SampleDocument.RC4_128_DISPLAY_NAME)
                        },
                    )
                }
            }
            Button(onClick = onPrint, enabled = state.canPrint) { Text("Print") }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            Button(onClick = onPrevious, enabled = state.pageIndex > 0) { Text("Previous") }
            Button(onClick = onNext, enabled = state.pageIndex + 1 < state.pageCount) { Text("Next") }
            Text(if (state.pageCount == 0) "No pages" else "Page ${state.pageIndex + 1} of ${state.pageCount}")
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onZoomOut, enabled = state.pageCount > 0 && state.zoomFactor > MIN_ZOOM_FACTOR) { Text("Zoom out") }
            Text("$zoomPercentage%", modifier = Modifier.semantics { contentDescription = "Zoom level: $zoomPercentage%" })
            TextButton(onClick = onZoomIn, enabled = state.pageCount > 0 && state.zoomFactor < MAX_ZOOM_FACTOR) { Text("Zoom in") }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(value = query, onValueChange = { query = it }, label = { Text("Find text") }, modifier = Modifier.weight(1f))
            Button(onClick = { onSearch(query) }, enabled = state.pageCount > 0) { Text("Find") }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(onClick = onPreviousMatch, enabled = state.searchHits.isNotEmpty()) { Text("Previous match") }
            TextButton(onClick = onNextMatch, enabled = state.searchHits.isNotEmpty()) { Text("Next match") }
        }
        if (state.isLoading) CircularProgressIndicator(modifier = Modifier.size(28.dp))
        Box(modifier = Modifier.fillMaxWidth().weight(1f)) {
            PageList(
                state = state,
                onPositionChanged = onPositionChanged,
                onScrollTargetConsumed = onScrollTargetConsumed,
                modifier = Modifier.fillMaxSize(),
            )
            // Mirrors the WinUI empty state and the GTK4 shell's overlay mark:
            // shown while the page area has nothing to display, hidden once a
            // document with pages is open.
            if (state.pageCount == 0) {
                Image(
                    painter = painterResource(R.drawable.ic_app_mark),
                    contentDescription = null,
                    modifier = Modifier.align(Alignment.Center).size(96.dp),
                )
            }
        }
        Text(state.status, style = MaterialTheme.typography.bodyMedium)
    }
    if (state.needsPassword) {
        var passwordVisible by remember { mutableStateOf(false) }
        val cancel = { onPasswordCancel(); password = "" }
        AlertDialog(
            onDismissRequest = cancel,
            title = { Text("Password required") },
            text = {
                Column {
                    state.passwordMessage?.let { Text(it) }
                    OutlinedTextField(
                        value = password,
                        onValueChange = { password = it },
                        label = { Text("Password") },
                        visualTransformation = if (passwordVisible) VisualTransformation.None else PasswordVisualTransformation(),
                        trailingIcon = {
                            TextButton(onClick = { passwordVisible = !passwordVisible }) {
                                Text(if (passwordVisible) "Hide" else "Show")
                            }
                        },
                    )
                }
            },
            confirmButton = { Button(onClick = { onPassword(password); password = "" }) { Text("Open") } },
            dismissButton = { TextButton(onClick = cancel) { Text("Cancel") } },
        )
    }
}
