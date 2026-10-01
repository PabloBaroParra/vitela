package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.text.KeyboardOptions
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
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.core.AnnotationPoint

/** What Edit content asks of the ViewModel, bundled like [FormFieldActions]. */
internal class ContentEditActions(
    val onToggle: () -> Unit,
    /** The reader laid out a page while the mode is armed. */
    val onPageShown: (pageIndex: Int) -> Unit,
    /** pageIndex, tap point and reach, in PDF points. */
    val onTap: (Int, AnnotationPoint, Double) -> Unit,
    /** documentId, text: the document is the one the dialog was built for. */
    val onRetype: (Long, String) -> Unit,
    /** documentId: deletes the dialog's run; the document is the one the dialog was built for. */
    val onDeleteText: (Long) -> Unit,
    /** documentId: swaps the retype dialog for a move armed on the next page tap. */
    val onMoveText: (Long) -> Unit,
    val onDismiss: () -> Unit,
    /** documentId, width, height as typed, in points: the document is the one the dialog was built for. */
    val onResize: (Long, String, String) -> Unit,
    val onDismissResizer: () -> Unit,
    /** documentId: swaps the resize dialog for a move armed on the next page tap. */
    val onMove: (Long) -> Unit,
    /** Disarms a move of a run or an image. */
    val onCancelMove: () -> Unit,
    /** documentId: deletes the dialog's image; the document is the one the dialog was built for. */
    val onDelete: (Long) -> Unit,
    /** documentId: checks the dialog's image can be replaced, then opens the image picker. */
    val onReplace: (Long) -> Unit,
    /** Arms the next page tap to place a new line of text. */
    val onAddText: () -> Unit,
    /** Opens the image picker; the chosen image arms the next page tap. */
    val onAddImage: () -> Unit,
    val onCancelInsert: () -> Unit,
    /** documentId, text, size as typed, in points: the document is the one the dialog was built for. */
    val onInsertText: (Long, String, String) -> Unit,
    val onDismissInserter: () -> Unit,
)

/**
 * The retype dialog: the run's text, ready to change. Keyed on the editor, so
 * a refusal — which comes back with what was typed — reopens with it rather
 * than with the run's old text. Move leaves it for a page tap that places the
 * run, like an image's Move. Delete takes the run off the page at once, like
 * an image's Delete: it is one undoable entry.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun TextRunEditorDialog(editor: TextRunEditor, documentId: Long, actions: ContentEditActions) {
    var text by remember(editor) { mutableStateOf(editor.text) }
    AlertDialog(
        onDismissRequest = actions.onDismiss,
        title = { Text("Edit text") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (editor.run.substitutesFont) Text(FONT_SUBSTITUTED, style = MaterialTheme.typography.bodySmall)
                Text("Move places this text where you tap next, keeping its font and size; save first if it already has a pending edit. Delete takes it off the page; Undo puts it back. It is not a secure redaction.", style = MaterialTheme.typography.bodySmall)
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
        dismissButton = {
            // Three actions beside Retype do not fit one row on a narrow phone.
            FlowRow(horizontalArrangement = Arrangement.End) {
                TextButton(onClick = { actions.onDeleteText(documentId) }) { Text("Delete") }
                TextButton(onClick = { actions.onMoveText(documentId) }) { Text("Move") }
                TextButton(onClick = actions.onDismiss) { Text("Cancel") }
            }
        },
    )
}

/**
 * The resize dialog: the image's width and height in points, ready to change.
 * Its top-left corner stays put. Keyed on the resizer, so a refusal reopens
 * with what was typed, like the retype dialog. Move leaves it for a page tap
 * that places the image, the way a form field is moved. Replace opens a picker
 * for a PNG or JPEG that fills the same box. Delete takes the image off the
 * page at once, with no second question: it is one undoable entry.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun ImageResizerDialog(resizer: ImageResizer, documentId: Long, actions: ContentEditActions) {
    var width by remember(resizer) { mutableStateOf(resizer.width) }
    var height by remember(resizer) { mutableStateOf(resizer.height) }
    AlertDialog(
        onDismissRequest = actions.onDismissResizer,
        title = { Text("Edit image") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("The image keeps its top-left corner and is stretched to the size you enter. Move places it where you tap next, keeping its size. Replace swaps its picture for a PNG or JPEG, stretched to the same box; save first if the image already has a pending edit. Delete takes it off the page; Undo puts it back.", style = MaterialTheme.typography.bodySmall)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PointsField("Width (pt)", width, { width = it }, resizer.error != null, Modifier.weight(1f))
                    PointsField("Height (pt)", height, { height = it }, resizer.error != null, Modifier.weight(1f))
                }
                resizer.error?.let { error -> Text(error, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            }
        },
        confirmButton = { Button(onClick = { actions.onResize(documentId, width, height) }) { Text("Resize") } },
        dismissButton = {
            // Four actions beside Resize do not fit one row on a phone.
            FlowRow(horizontalArrangement = Arrangement.End) {
                TextButton(onClick = { actions.onDelete(documentId) }) { Text("Delete") }
                TextButton(onClick = { actions.onReplace(documentId) }) { Text("Replace") }
                TextButton(onClick = { actions.onMove(documentId) }) { Text("Move") }
                TextButton(onClick = actions.onDismissResizer) { Text("Cancel") }
            }
        },
    )
}

/**
 * The insert dialog: a new line and its size, for the spot tapped. Keyed on
 * the inserter, so a refusal reopens with what was typed, like the retype
 * dialog.
 */
@Composable
internal fun TextInserterDialog(inserter: TextInserter, documentId: Long, actions: ContentEditActions) {
    var text by remember(inserter) { mutableStateOf(inserter.text) }
    var size by remember(inserter) { mutableStateOf(inserter.size) }
    AlertDialog(
        onDismissRequest = actions.onDismissInserter,
        title = { Text("Add text — page ${inserter.pageIndex + 1}") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Adds one line of Helvetica text as page content, its top-left corner where you tapped. Some characters cannot be shown in this font.", style = MaterialTheme.typography.bodySmall)
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    label = { Text("Text") },
                    singleLine = true,
                    isError = inserter.error != null,
                )
                PointsField("Size (pt)", size, { size = it }, inserter.error != null, Modifier)
                inserter.error?.let { error -> Text(error, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            }
        },
        confirmButton = { Button(onClick = { actions.onInsertText(documentId, text, size) }) { Text("Insert") } },
        dismissButton = { TextButton(onClick = actions.onDismissInserter) { Text("Cancel") } },
    )
}

@Composable
private fun PointsField(label: String, value: String, onValueChange: (String) -> Unit, isError: Boolean, modifier: Modifier) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(label) },
        singleLine = true,
        isError = isError,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
        modifier = modifier,
    )
}
