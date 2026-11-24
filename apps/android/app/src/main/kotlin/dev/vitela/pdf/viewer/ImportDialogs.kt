package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
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
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R

/**
 * The two questions an import can raise, both before any page goes in: an
 * added PDF's password, and whether to accept what the pages would lose.
 * Cancel is the prominent answer to the second, as on Windows: accepting is
 * the deliberate one.
 */
@Composable
internal fun ImportDialogs(organize: OrganizeState, actions: OrganizeActions) {
    organize.importPassword?.let { ImportPasswordDialog(it, actions) }
    if (organize.importWarnings.isNotEmpty() && organize.importPassword == null) {
        AlertDialog(
            onDismissRequest = actions.onImportWarningsDismiss,
            title = { Text(stringResource(R.string.import_warning_title)) },
            text = {
                Column(modifier = Modifier.heightIn(max = 320.dp).verticalScroll(rememberScrollState())) {
                    organize.importWarnings.forEach { Text("• $it") }
                    Text(stringResource(R.string.import_warning_detail))
                }
            },
            confirmButton = { TextButton(onClick = actions.onImportWarningsAccept) { Text(stringResource(R.string.import_anyway)) } },
            dismissButton = { Button(onClick = actions.onImportWarningsDismiss) { Text(stringResource(R.string.dialog_cancel)) } },
        )
    }
}

/**
 * The password lives in this dialog alone, never in the state: keyed on the
 * prompt, so a wrong one is cleared when the core refuses it and the next file
 * starts empty.
 */
@Composable
private fun ImportPasswordDialog(prompt: ImportPasswordPrompt, actions: OrganizeActions) {
    var password by remember(prompt) { mutableStateOf("") }
    var visible by remember(prompt) { mutableStateOf(false) }
    AlertDialog(
        onDismissRequest = actions.onImportPasswordCancel,
        title = { Text(stringResource(R.string.password_title)) },
        text = {
            Column {
                Text(stringResource(if (prompt.wrong) R.string.import_password_wrong else R.string.import_password_prompt, prompt.name))
                OutlinedTextField(
                    value = password,
                    onValueChange = { password = it },
                    label = { Text(stringResource(R.string.password_label)) },
                    visualTransformation = if (visible) VisualTransformation.None else PasswordVisualTransformation(),
                    trailingIcon = { TextButton(onClick = { visible = !visible }) { Text(if (visible) stringResource(R.string.password_hide) else stringResource(R.string.password_show)) } },
                )
            }
        },
        confirmButton = { Button(onClick = { actions.onImportPassword(password) }) { Text(stringResource(R.string.dialog_add)) } },
        dismissButton = { TextButton(onClick = actions.onImportPasswordCancel) { Text(stringResource(R.string.dialog_cancel)) } },
    )
}
