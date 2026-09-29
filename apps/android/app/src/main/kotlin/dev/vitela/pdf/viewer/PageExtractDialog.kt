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

/** Extract pages: which pages go into the new PDF. It stays open on a refused range and says why. */
@Composable
internal fun PageExtractDialog(
    editor: PageExtractEditor,
    onChange: (String) -> Unit,
    onExtract: () -> Unit,
    onDismiss: () -> Unit,
) {
    val enabled = editor.extractAllowed
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Extract pages to a new PDF") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                editor.message?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
                OutlinedTextField(
                    value = editor.range,
                    onValueChange = onChange,
                    label = { Text("Pages") },
                    placeholder = { Text("e.g. 1-3,7") },
                    enabled = enabled,
                    singleLine = true,
                )
            }
        },
        confirmButton = { Button(onClick = onExtract, enabled = enabled) { Text("Extract") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
