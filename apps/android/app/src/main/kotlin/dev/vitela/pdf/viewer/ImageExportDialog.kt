package dev.vitela.pdf.viewer

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.core.ImageExportFormat

/** Export images: which pages, PNG or JPEG, and at what resolution. It stays open on a refused choice and says why. */
@Composable
internal fun ImageExportDialog(
    editor: ImageExportEditor,
    onChange: (ImageExportDraft) -> Unit,
    onExport: () -> Unit,
    onDismiss: () -> Unit,
) {
    val draft = editor.draft
    val enabled = editor.exportAllowed
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Export pages as images") },
        text = {
            // Scrolls so the last field stays reachable above the keyboard.
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.verticalScroll(rememberScrollState())) {
                editor.message?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
                Text("Pages", style = MaterialTheme.typography.labelLarge)
                Choice("All pages", draft.pages == ImageExportPages.All, enabled) { onChange(draft.copy(pages = ImageExportPages.All)) }
                Choice("Current page", draft.pages == ImageExportPages.Current, enabled) { onChange(draft.copy(pages = ImageExportPages.Current)) }
                Choice("Custom range", draft.pages == ImageExportPages.Custom, enabled) { onChange(draft.copy(pages = ImageExportPages.Custom)) }
                OutlinedTextField(
                    value = draft.customRange,
                    // Typing a range means a range: no need to pick the choice first.
                    onValueChange = { onChange(draft.copy(pages = ImageExportPages.Custom, customRange = it)) },
                    label = { Text("e.g. 1-3,7") },
                    enabled = enabled,
                    singleLine = true,
                )
                Text("Format", style = MaterialTheme.typography.labelLarge)
                Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    ImageExportFormat.entries.forEach { format ->
                        Choice(format.label, draft.format == format, enabled) { onChange(draft.copy(format = format)) }
                    }
                }
                OutlinedTextField(
                    value = draft.dpi,
                    onValueChange = { text -> onChange(draft.copy(dpi = text.filter(Char::isDigit).take(3))) },
                    label = { Text("Resolution ($MIN_EXPORT_DPI-$MAX_EXPORT_DPI DPI)") },
                    enabled = enabled,
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                )
            }
        },
        confirmButton = { Button(onClick = onExport, enabled = enabled) { Text("Export") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

@Composable
private fun Choice(label: String, selected: Boolean, enabled: Boolean, onSelect: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.clickable(enabled = enabled, role = Role.RadioButton, onClick = onSelect),
    ) {
        RadioButton(selected = selected, onClick = null, enabled = enabled)
        Text(label, modifier = Modifier.padding(start = 8.dp))
    }
}
