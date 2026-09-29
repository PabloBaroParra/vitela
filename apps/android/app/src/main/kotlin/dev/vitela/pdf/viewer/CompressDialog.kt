package dev.vitela.pdf.viewer

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.core.CompressPreset

/**
 * Compress PDF: one radio per preset, Balanced pre-selected. No predicted
 * saving — nothing can answer that without running. A signed file says so
 * here and the button reads "Compress anyway", so confirming is the consent.
 */
@Composable
internal fun CompressDialog(
    editor: CompressEditor,
    onSelect: (CompressPreset) -> Unit,
    onCompress: () -> Unit,
    onDismiss: () -> Unit,
) {
    val enabled = editor.refusal == null
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Compress PDF") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.verticalScroll(rememberScrollState())) {
                editor.refusal?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
                Text("Writes a smaller copy. The open document stays as it is.", style = MaterialTheme.typography.bodyMedium)
                CompressPreset.entries.forEach { preset ->
                    val (name, description) = describePreset(preset)
                    Row(
                        verticalAlignment = Alignment.Top,
                        modifier = Modifier.clickable(enabled = enabled, role = Role.RadioButton) { onSelect(preset) },
                    ) {
                        RadioButton(selected = editor.preset == preset, onClick = null, enabled = enabled)
                        Column(modifier = Modifier.padding(start = 8.dp)) {
                            Text(name, style = MaterialTheme.typography.labelLarge)
                            Text(description, style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
                if (editor.signaturesWillBreak) {
                    Text(
                        "This document is signed. Compressing rewrites the file, so the compressed copy's digital signature will no longer verify.",
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
        },
        confirmButton = {
            Button(onClick = onCompress, enabled = enabled) { Text(if (editor.signaturesWillBreak) "Compress anyway" else "Compress") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
