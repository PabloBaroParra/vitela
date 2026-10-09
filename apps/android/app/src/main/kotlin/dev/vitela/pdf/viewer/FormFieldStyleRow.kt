package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.core.AnnotationColor
import dev.vitela.pdf.core.FieldFont
import dev.vitela.pdf.core.FieldTextStyle

/** The text colors a field can be given from its row; a color another tool wrote shows as Custom. */
internal val FIELD_COLORS = listOf(
    "Black" to AnnotationColor(0, 0, 0),
    "Blue" to AnnotationColor(0, 70, 200),
    "Red" to AnnotationColor(220, 40, 40),
    "Green" to AnnotationColor(0, 128, 0),
)

internal val FieldFont.label: String get() = when (this) {
    FieldFont.Helvetica -> "Helvetica"
    FieldFont.TimesRoman -> "Times"
    FieldFont.Courier -> "Courier"
}

internal fun fieldColorLabel(color: AnnotationColor): String = FIELD_COLORS.firstOrNull { it.second == color }?.first ?: "Custom"

/**
 * A field's name, typed. It commits only when focus leaves, and Done lets
 * focus go rather than committing itself: a rename is not optimistic, so a
 * Done and the focus loss after it would otherwise queue the same rename
 * twice — two undo steps for one edit — before the core has answered.
 */
@Composable
internal fun FieldNameBox(name: String, rename: (String) -> Unit) {
    // Re-seeded when the core's name changes under the row: an undo, or another rename.
    var draft by remember(name) { mutableStateOf(name) }
    var focused by remember { mutableStateOf(false) }
    val focusManager = LocalFocusManager.current
    OutlinedTextField(
        value = draft,
        onValueChange = { draft = it },
        label = { Text("Field name") },
        singleLine = true,
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { focusManager.clearFocus() }),
        modifier = Modifier.fillMaxWidth().releaseFocusOnEnter().onFocusChanged {
            if (focused && !it.isFocused) rename(draft)
            focused = it.isFocused
        },
    )
}

/**
 * A field's font, size and text color. Each menu pick is one edit; the size
 * commits when focus leaves its box, like the field's width and height.
 */
@Composable
internal fun FieldStyleRow(style: FieldTextStyle, restyle: (FieldTextStyle) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        StyleMenu(style.font.label, FieldFont.entries.map { it.label to it }) { restyle(style.copy(font = it)) }
        FontSizeBox(style.sizePt, Modifier.weight(1f)) { restyle(style.copy(sizePt = it)) }
        StyleMenu(fieldColorLabel(style.color), FIELD_COLORS) { restyle(style.copy(color = it)) }
    }
}

@Composable
private fun <T> StyleMenu(current: String, choices: List<Pair<String, T>>, choose: (T) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        OutlinedButton(onClick = { expanded = true }) { Text(current) }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            choices.forEach { (label, value) ->
                DropdownMenuItem(text = { Text(label) }, onClick = { expanded = false; choose(value) })
            }
        }
    }
}

@Composable
private fun FontSizeBox(sizePt: Double, modifier: Modifier, commit: (Double) -> Unit) {
    // Re-seeded when the core's size changes under the row: an undo, or another restyle.
    // A refused size stays typed, next to the status that says why.
    var draft by remember(sizePt) { mutableStateOf(pointsText(sizePt)) }
    var focused by remember { mutableStateOf(false) }
    val focusManager = LocalFocusManager.current
    OutlinedTextField(
        value = draft,
        onValueChange = { draft = it },
        label = { Text("Size (pt)") },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal, imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { focusManager.clearFocus() }),
        modifier = modifier.releaseFocusOnEnter().onFocusChanged {
            if (focused && !it.isFocused) commit(typedPoints(draft))
            focused = it.isFocused
        },
    )
}
