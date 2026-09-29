package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.core.FormField
import dev.vitela.pdf.core.FormFieldKind
import dev.vitela.pdf.core.FormFieldValue

/** What the Form fields panel can ask of the ViewModel; one value so the screen's parameter list stays short. */
internal class FormFieldActions(
    val onToggle: () -> Unit,
    /** documentId, fieldId, value: the document is the one the row was built for. */
    val onFill: (Long, Long, FormFieldValue) -> Unit,
)

/**
 * Form fields: one row per field, below the reader so the page being filled
 * stays in sight. Each row is built so it cannot *offer* an invalid value — a
 * capped length, the real options — and the core still validates what arrives.
 *
 * A text row commits on Done and when it loses focus, never per keystroke: one
 * fill is one undo step and one preview rebuild.
 */
@Composable
internal fun FormFieldsPanel(panel: FormFieldsState, documentId: Long, actions: FormFieldActions, modifier: Modifier = Modifier) {
    val fill = { fieldId: Long, value: FormFieldValue -> actions.onFill(documentId, fieldId, value) }
    val focusManager = LocalFocusManager.current
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Form fields", style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
            // Focus leaves first, so a field still being typed in commits
            // while the panel is there to take it.
            TextButton(onClick = { focusManager.clearFocus(); actions.onToggle() }) { Text("Close") }
        }
        formFieldsNotice(panel)?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
        LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            // Keyed by document too: a row's remembered draft must not survive into another file's field of the same id.
            items(panel.fields, key = { "$documentId:${it.id}" }) { field ->
                FormFieldRow(field, panel.fillAllowed, fill)
            }
        }
    }
}

@Composable
private fun FormFieldRow(field: FormField, enabled: Boolean, fill: (Long, FormFieldValue) -> Unit) {
    when (val kind = field.kind) {
        is FormFieldKind.Text -> CommitOnLeaveField(
            label = field.name,
            value = (field.value as? FormFieldValue.Text)?.text.orEmpty(),
            enabled = enabled && !kind.readOnly,
            multiline = kind.multiline,
            maxLength = kind.maxLength,
        ) { fill(field.id, FormFieldValue.Text(it)) }
        FormFieldKind.Checkbox -> {
            val checked = (field.value as? FormFieldValue.Checked)?.checked == true
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth().toggleable(checked, enabled, Role.Checkbox) { fill(field.id, FormFieldValue.Checked(it)) },
            ) {
                Checkbox(checked = checked, onCheckedChange = null, enabled = enabled)
                Text(field.name)
            }
        }
        is FormFieldKind.RadioGroup -> Column {
            Text(field.name, style = MaterialTheme.typography.labelLarge)
            val chosen = (field.value as? FormFieldValue.Choice)?.option
            kind.options.forEach { option ->
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth().selectable(option == chosen, enabled, Role.RadioButton) { fill(field.id, FormFieldValue.Choice(option)) },
                ) {
                    RadioButton(selected = option == chosen, onClick = null, enabled = enabled)
                    Text(option)
                }
            }
        }
        // An editable dropdown accepts text outside its options, so it is typed
        // into like a text field — the GTK and Windows shells' choice too.
        is FormFieldKind.Dropdown -> if (kind.editable) {
            CommitOnLeaveField(
                label = field.name,
                value = (field.value as? FormFieldValue.Choice)?.option.orEmpty(),
                enabled = enabled,
                placeholder = kind.options.joinToString(", "),
            ) { fill(field.id, editableChoice(it)) }
        } else {
            ChoiceMenu(field, kind.options, enabled, fill)
        }
    }
}

/** A text field that commits what was typed when the reader leaves it, or presses Done on a single line. */
@Composable
private fun CommitOnLeaveField(
    label: String,
    value: String,
    enabled: Boolean,
    multiline: Boolean = false,
    maxLength: Int? = null,
    placeholder: String? = null,
    commit: (String) -> Unit,
) {
    // Re-seeded when the core's value changes under the row: an undo, or a refused fill put back.
    var draft by remember(value) { mutableStateOf(value) }
    var focused by remember { mutableStateOf(false) }
    OutlinedTextField(
        value = draft,
        onValueChange = { draft = cappedText(it, maxLength) },
        label = { Text(label) },
        placeholder = placeholder?.let { { Text(it) } },
        enabled = enabled,
        singleLine = !multiline,
        keyboardOptions = KeyboardOptions(imeAction = if (multiline) ImeAction.Default else ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { commit(draft) }),
        modifier = Modifier.fillMaxWidth().onFocusChanged {
            if (focused && !it.isFocused) commit(draft)
            focused = it.isFocused
        },
    )
}

@Composable
private fun ChoiceMenu(field: FormField, options: List<String>, enabled: Boolean, fill: (Long, FormFieldValue) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Column {
        Text(field.name, style = MaterialTheme.typography.labelLarge)
        Box {
            OutlinedButton(onClick = { expanded = true }, enabled = enabled) {
                Text(choiceLabel(options, (field.value as? FormFieldValue.Choice)?.option))
            }
            DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                // "Nothing chosen" is an entry the reader can pick, not a state
                // reachable only by never touching the menu.
                (listOf<String?>(null) + options).forEach { option ->
                    DropdownMenuItem(
                        text = { Text(option ?: FORM_NO_CHOICE) },
                        onClick = {
                            expanded = false
                            fill(field.id, FormFieldValue.Choice(option))
                        },
                    )
                }
            }
        }
    }
}
