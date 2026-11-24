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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.core.CompressPreset
import dev.vitela.pdf.text.resolve

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
        title = { Text(stringResource(R.string.compress_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.verticalScroll(rememberScrollState())) {
                editor.refusal?.let { Text(it.resolve(), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
                Text(stringResource(R.string.compress_detail), style = MaterialTheme.typography.bodyMedium)
                CompressPreset.entries.forEach { preset ->
                    val (name, description) = describePreset(preset)
                    Row(
                        verticalAlignment = Alignment.Top,
                        modifier = Modifier.clickable(enabled = enabled, role = Role.RadioButton) { onSelect(preset) },
                    ) {
                        RadioButton(selected = editor.preset == preset, onClick = null, enabled = enabled)
                        Column(modifier = Modifier.padding(start = 8.dp)) {
                            Text(name.resolve(), style = MaterialTheme.typography.labelLarge)
                            Text(description.resolve(), style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
                if (editor.signaturesWillBreak) {
                    Text(
                        stringResource(R.string.compress_signed),
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
        },
        confirmButton = {
            Button(onClick = onCompress, enabled = enabled) { Text(if (editor.signaturesWillBreak) stringResource(R.string.compress_anyway) else stringResource(R.string.compress_confirm)) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.dialog_cancel)) } },
    )
}
