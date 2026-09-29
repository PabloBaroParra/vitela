package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.dp

/** Split PDF: the pages each new file starts after. It stays open on refused cuts and says why. */
@Composable
internal fun PageSplitDialog(
    editor: PageSplitEditor,
    pageCount: Int,
    onChange: (String) -> Unit,
    onSplit: () -> Unit,
    onDismiss: () -> Unit,
) {
    val enabled = editor.splitAllowed
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Split PDF") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                editor.message?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
                OutlinedTextField(
                    value = editor.cuts,
                    onValueChange = onChange,
                    label = { Text("Split after page") },
                    placeholder = { Text("e.g. 3,7") },
                    enabled = enabled,
                    singleLine = true,
                )
                Text("This document has $pageCount pages. Each cut starts a new file.", style = MaterialTheme.typography.bodyMedium)
            }
        },
        confirmButton = { Button(onClick = onSplit, enabled = enabled) { Text("Split") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
