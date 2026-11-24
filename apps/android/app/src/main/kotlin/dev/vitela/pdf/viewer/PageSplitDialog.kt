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
        title = { Text(stringResource(R.string.split_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                editor.message?.let { Text(it.resolve(), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
                OutlinedTextField(
                    value = editor.cuts,
                    onValueChange = onChange,
                    label = { Text(stringResource(R.string.split_after)) },
                    placeholder = { Text(stringResource(R.string.split_example)) },
                    enabled = enabled,
                    singleLine = true,
                )
                Text(stringResource(R.string.split_detail, pageCount), style = MaterialTheme.typography.bodyMedium)
            }
        },
        confirmButton = { Button(onClick = onSplit, enabled = enabled) { Text(stringResource(R.string.split_confirm)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.dialog_cancel)) } },
    )
}
