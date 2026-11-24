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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.core.DocumentInfo
import dev.vitela.pdf.text.resolve

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
        title = { Text(stringResource(R.string.metadata_title)) },
        text = {
            // Scrolls so the last field stays reachable above the keyboard.
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.verticalScroll(rememberScrollState())) {
                editor.message?.let { Text(it.resolve(), style = MaterialTheme.typography.bodyMedium) }
                MetadataField(stringResource(R.string.metadata_field_title), draft.title, editor.editingAllowed) { onChange(draft.copy(title = it)) }
                MetadataField(stringResource(R.string.metadata_field_author), draft.author, editor.editingAllowed) { onChange(draft.copy(author = it)) }
                MetadataField(stringResource(R.string.metadata_field_subject), draft.subject, editor.editingAllowed) { onChange(draft.copy(subject = it)) }
                MetadataField(stringResource(R.string.metadata_field_keywords), draft.keywords, editor.editingAllowed) { onChange(draft.copy(keywords = it)) }
                MetadataField(stringResource(R.string.metadata_field_creator), draft.creator, editor.editingAllowed) { onChange(draft.copy(creator = it)) }
                MetadataField(stringResource(R.string.metadata_field_producer), draft.producer, editor.editingAllowed) { onChange(draft.copy(producer = it)) }
            }
        },
        confirmButton = { Button(onClick = onApply, enabled = editor.editingAllowed) { Text(stringResource(R.string.metadata_apply)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(if (editor.editingAllowed) stringResource(R.string.dialog_cancel) else stringResource(R.string.dialog_close)) } },
    )
}

@Composable
private fun MetadataField(label: String, value: String?, enabled: Boolean, onValueChange: (String) -> Unit) {
    OutlinedTextField(value = value.orEmpty(), onValueChange = onValueChange, label = { Text(label) }, enabled = enabled, singleLine = true)
}
