package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.text.resolve

/** What the Sign dialog can ask of the ViewModel and the shell's certificate picker. */
data class SignActions(
    val onOpen: () -> Unit,
    val onChooseCertificate: () -> Unit,
    val onUnlock: (password: String) -> Unit,
    val onSelectIdentity: (id: String) -> Unit,
    val onSign: () -> Unit,
    val onDismiss: () -> Unit,
)

/**
 * Sign with a certificate file, one step at a time: pick it, unlock it, pick
 * who to sign as. The password lives in this dialog's own field and leaves
 * it only through [SignActions.onUnlock], and is keyed to the file, so
 * picking another file starts from an empty field.
 */
@Composable
internal fun SignDialog(editor: SignEditor, actions: SignActions) {
    var password by remember(editor.certificateName) { mutableStateOf("") }
    val unlocked = editor.identities.isNotEmpty()
    AlertDialog(
        onDismissRequest = actions.onDismiss,
        title = { Text(stringResource(R.string.sign_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.verticalScroll(rememberScrollState())) {
                editor.refusal?.let {
                    Text(it.resolve(), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium)
                    return@Column
                }
                Text(
                    stringResource(R.string.sign_detail),
                    style = MaterialTheme.typography.bodyMedium,
                )
                OutlinedButton(onClick = actions.onChooseCertificate, enabled = !editor.unlocking) {
                    Text(editor.certificateName?.let { stringResource(R.string.sign_certificate, it) } ?: stringResource(R.string.sign_choose_certificate))
                }
                if (editor.certificateName != null && !unlocked) {
                    OutlinedTextField(
                        value = password,
                        onValueChange = { password = it },
                        label = { Text(stringResource(R.string.sign_certificate_password)) },
                        enabled = !editor.unlocking,
                        singleLine = true,
                        visualTransformation = PasswordVisualTransformation(),
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                    )
                }
                if (unlocked) {
                    Text(stringResource(R.string.sign_identity), style = MaterialTheme.typography.titleSmall)
                    editor.identities.forEach { identity ->
                        val selected = identity.id == editor.selectedIdentityId
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier.selectable(selected = selected, role = Role.RadioButton) { actions.onSelectIdentity(identity.id) },
                        ) {
                            RadioButton(selected = selected, onClick = null)
                            Text(identity.name, style = MaterialTheme.typography.bodyMedium)
                        }
                    }
                }
                editor.error?.let { Text(it.resolve(), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
            }
        },
        confirmButton = {
            when {
                editor.refusal != null -> Unit
                unlocked -> Button(onClick = actions.onSign, enabled = editor.selectedIdentityId != null) { Text(stringResource(R.string.sign_confirm)) }
                else -> Button(
                    onClick = { actions.onUnlock(password) },
                    enabled = editor.certificateName != null && !editor.unlocking,
                ) { Text(if (editor.unlocking) stringResource(R.string.sign_unlocking) else stringResource(R.string.sign_unlock)) }
            }
        },
        dismissButton = { TextButton(onClick = actions.onDismiss) { Text(if (editor.refusal != null) stringResource(R.string.dialog_close) else stringResource(R.string.dialog_cancel)) } },
    )
}
