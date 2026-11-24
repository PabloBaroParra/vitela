package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.text.resolve

/**
 * Protect with a password: both roles, as on Windows. The passwords live in
 * this dialog's own fields and leave it only through [onProtect], so closing
 * the dialog forgets them. A signed file says so here and the button reads
 * "Protect anyway", so confirming is the consent.
 */
@Composable
internal fun ProtectDialog(
    editor: ProtectEditor,
    onProtect: (openPassword: String, permissionsPassword: String) -> Unit,
    onDismiss: () -> Unit,
) {
    var openPassword by remember { mutableStateOf("") }
    var permissionsPassword by remember { mutableStateOf("") }
    val enabled = editor.refusal == null
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.protect_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.verticalScroll(rememberScrollState())) {
                editor.refusal?.let { Text(it.resolve(), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
                PasswordField(stringResource(R.string.protect_open_password), openPassword, enabled) { openPassword = it }
                PasswordField(stringResource(R.string.protect_permissions_password), permissionsPassword, enabled) { permissionsPassword = it }
                Text(stringResource(R.string.protect_detail), style = MaterialTheme.typography.bodyMedium)
                if (editor.signaturesWillBreak) {
                    Text(
                        stringResource(R.string.protect_signed),
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
                editor.error?.let { Text(it.resolve(), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
            }
        },
        confirmButton = {
            Button(onClick = { onProtect(openPassword, permissionsPassword) }, enabled = enabled) {
                Text(if (editor.signaturesWillBreak) stringResource(R.string.protect_anyway) else stringResource(R.string.protect_confirm))
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.dialog_cancel)) } },
    )
}

@Composable
private fun PasswordField(label: String, value: String, enabled: Boolean, onValueChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(label) },
        enabled = enabled,
        singleLine = true,
        visualTransformation = PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
    )
}
