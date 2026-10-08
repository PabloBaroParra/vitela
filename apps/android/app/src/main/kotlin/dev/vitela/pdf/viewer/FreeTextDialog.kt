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
 * Asks for a text box's text, or for the new text of one already placed.
 * Multi-line; the confirm button stays disabled while the text is blank, and the
 * text is sent exactly as typed. The text lives here, keyed by the draft's
 * target: a refusal from the core only changes [FreeTextDraft.error], so the
 * field keeps what the reader typed. [documentId] travels with the answer so
 * a dialog left over from a replaced document cannot change the new one.
 */
@Composable
internal fun FreeTextDialog(draft: FreeTextDraft, documentId: Long, onConfirm: (Long, String) -> Unit, onCancel: () -> Unit) {
    var text by remember(draft.pageIndex, draft.target) { mutableStateOf(freeTextInitialText(draft)) }
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text(freeTextDialogTitle(draft)) },
        text = {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                label = { Text("Text") },
                isError = draft.error != null,
                supportingText = draft.error?.let { message -> { Text(message) } },
                minLines = 3,
                maxLines = 8,
                modifier = Modifier.heightIn(min = 120.dp),
            )
        },
        confirmButton = { Button(onClick = { onConfirm(documentId, text) }, enabled = freeTextConfirmEnabled(text, draft.busy)) { Text(freeTextConfirmLabel(draft)) } },
        dismissButton = { TextButton(onClick = onCancel, enabled = !draft.busy) { Text("Cancel") } },
    )
}
