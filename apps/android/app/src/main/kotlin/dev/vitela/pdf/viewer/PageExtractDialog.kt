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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.text.resolve

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
        title = { Text(stringResource(R.string.extract_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                editor.message?.let { Text(it.resolve(), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
                OutlinedTextField(
                    value = editor.range,
                    onValueChange = onChange,
                    label = { Text(stringResource(R.string.dialog_pages)) },
                    placeholder = { Text(stringResource(R.string.dialog_pages_example)) },
                    enabled = enabled,
                    singleLine = true,
                )
            }
        },
        confirmButton = { Button(onClick = onExtract, enabled = enabled) { Text(stringResource(R.string.extract_confirm)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.dialog_cancel)) } },
    )
}
