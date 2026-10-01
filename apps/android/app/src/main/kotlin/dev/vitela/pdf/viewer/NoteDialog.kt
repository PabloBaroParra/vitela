package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.heightIn
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
import androidx.compose.ui.unit.dp

/**
 * Asks for a Note's text after its rectangle was chosen. Multi-line; **Add**
 * stays disabled while the text is blank, and the text is sent exactly as
 * typed. [documentId] travels with the answer so a prompt left over from a
 * replaced document cannot add to the new one.
 */
@Composable
internal fun NoteDialog(placement: NotePlacement, documentId: Long, onAdd: (Long, String) -> Unit, onCancel: () -> Unit) {
    var text by remember(placement) { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text("Add note — page ${placement.pageIndex + 1}") },
        text = {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                label = { Text("Note text") },
                minLines = 4,
                maxLines = 10,
                modifier = Modifier.heightIn(min = 120.dp),
            )
        },
        confirmButton = { Button(onClick = { onAdd(documentId, text) }, enabled = text.isNotBlank()) { Text("Add") } },
        dismissButton = { TextButton(onClick = onCancel) { Text("Cancel") } },
    )
}

/**
 * **Read note**: the note's text in a read-only field — scrollable and
 * selectable for copying, never editable. Closing records nothing.
 */
@Composable
internal fun NoteReadingDialog(reading: NoteReading, onClose: () -> Unit) {
    AlertDialog(
        onDismissRequest = onClose,
        title = { Text("Note — page ${reading.pageIndex + 1}") },
        text = {
            OutlinedTextField(
                value = reading.contents,
                onValueChange = {},
                readOnly = true,
                label = { Text("Note text") },
                minLines = 4,
                maxLines = 10,
                modifier = Modifier.heightIn(min = 120.dp),
            )
        },
        confirmButton = { TextButton(onClick = onClose) { Text("Close") } },
    )
}
