package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.core.DocumentInfo

/** Document properties: the six text keys of `/Info`, read-only when the file forbids changes. */
@Composable
internal fun MetadataDialog(
    editor: MetadataEditor,
    onChange: (DocumentInfo) -> Unit,
    onApply: () -> Unit,
    onDismiss: () -> Unit,
) {
    val draft = editor.draft
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Document properties") },
        text = {
            // Scrolls so the last field stays reachable above the keyboard.
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.verticalScroll(rememberScrollState())) {
                editor.message?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
                MetadataField("Title", draft.title, editor.editingAllowed) { onChange(draft.copy(title = it)) }
                MetadataField("Author", draft.author, editor.editingAllowed) { onChange(draft.copy(author = it)) }
                MetadataField("Subject", draft.subject, editor.editingAllowed) { onChange(draft.copy(subject = it)) }
                MetadataField("Keywords", draft.keywords, editor.editingAllowed) { onChange(draft.copy(keywords = it)) }
                MetadataField("Creator", draft.creator, editor.editingAllowed) { onChange(draft.copy(creator = it)) }
                MetadataField("Producer", draft.producer, editor.editingAllowed) { onChange(draft.copy(producer = it)) }
            }
        },
        confirmButton = { Button(onClick = onApply, enabled = editor.editingAllowed) { Text("Apply") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(if (editor.editingAllowed) "Cancel" else "Close") } },
    )
}

@Composable
private fun MetadataField(label: String, value: String?, enabled: Boolean, onValueChange: (String) -> Unit) {
    OutlinedTextField(value = value.orEmpty(), onValueChange = onValueChange, label = { Text(label) }, enabled = enabled, singleLine = true)
}
