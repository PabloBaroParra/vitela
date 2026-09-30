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
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp

/** The two questions an import can raise: an added PDF's password, and what its pages lost on the way in. */
@Composable
internal fun ImportDialogs(organize: OrganizeState, actions: OrganizeActions) {
    organize.importPassword?.let { ImportPasswordDialog(it, actions) }
    if (organize.importWarnings.isNotEmpty() && organize.importPassword == null) {
        AlertDialog(
            onDismissRequest = actions.onImportWarningsDismiss,
            title = { Text("Some content changed on the way in") },
            text = {
                Column(modifier = Modifier.heightIn(max = 320.dp).verticalScroll(rememberScrollState())) {
                    organize.importWarnings.forEach { Text("• $it") }
                    Text("The pages were added. Undo removes them.")
                }
            },
            confirmButton = { Button(onClick = actions.onImportWarningsDismiss) { Text("OK") } },
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
        title = { Text("Password required") },
        text = {
            Column {
                Text(if (prompt.wrong) "That password does not open ${prompt.name}." else "${prompt.name} is protected with a password.")
                OutlinedTextField(
                    value = password,
                    onValueChange = { password = it },
                    label = { Text("Password") },
                    visualTransformation = if (visible) VisualTransformation.None else PasswordVisualTransformation(),
                    trailingIcon = { TextButton(onClick = { visible = !visible }) { Text(if (visible) "Hide" else "Show") } },
                )
            }
        },
        confirmButton = { Button(onClick = { actions.onImportPassword(password) }) { Text("Add") } },
        dismissButton = { TextButton(onClick = actions.onImportPasswordCancel) { Text("Cancel") } },
    )
}
