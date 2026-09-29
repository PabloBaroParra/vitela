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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.core.AnnotationPoint

/** What Edit text asks of the ViewModel, bundled like [FormFieldActions]. */
internal class ContentEditActions(
    val onToggle: () -> Unit,
    /** The reader laid out a page while the mode is armed. */
    val onPageShown: (pageIndex: Int) -> Unit,
    /** pageIndex, tap point and reach, in PDF points. */
    val onTap: (Int, AnnotationPoint, Double) -> Unit,
    /** documentId, text: the document is the one the dialog was built for. */
    val onRetype: (Long, String) -> Unit,
    val onDismiss: () -> Unit,
)

/**
 * The retype dialog: the run's text, ready to change. Keyed on the editor, so
 * a refusal — which comes back with what was typed — reopens with it rather
 * than with the run's old text.
 */
@Composable
internal fun TextRunEditorDialog(editor: TextRunEditor, documentId: Long, actions: ContentEditActions) {
    var text by remember(editor) { mutableStateOf(editor.text) }
    AlertDialog(
        onDismissRequest = actions.onDismiss,
        title = { Text("Edit text") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (editor.run.substitutesFont) Text(FONT_SUBSTITUTED, style = MaterialTheme.typography.bodySmall)
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    singleLine = true,
                    isError = editor.error != null,
                    supportingText = editor.error?.let { error -> { Text(error) } },
                )
            }
        },
        confirmButton = { Button(onClick = { actions.onRetype(documentId, text) }) { Text("Retype") } },
        dismissButton = { TextButton(onClick = actions.onDismiss) { Text("Cancel") } },
    )
}
