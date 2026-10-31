package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
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
 * The shape dialog: a table border, rule or shaded box's width and height in
 * points, ready to change. Its top-left corner stays put and its line width is
 * kept. A flat axis — a rule has no height — is shown and cannot be changed:
 * the core moves such an axis, never scales it. Keyed on the shaper, so a
 * refusal reopens with what was typed, like the image dialog. Move leaves it
 * for a page tap that places the shape; Delete takes it off the page at once,
 * with no second question: it is one undoable entry.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun PathShaperDialog(shaper: PathShaper, documentId: Long, actions: ContentEditActions) {
    var width by remember(shaper) { mutableStateOf(shaper.width) }
    var height by remember(shaper) { mutableStateOf(shaper.height) }
    AlertDialog(
        onDismissRequest = actions.onDismissShaper,
        title = { Text("Edit shape") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("The shape keeps its top-left corner and its line width, and is stretched to the size you enter; the text inside it is not touched. A line has no height to change. Move lets you drag it, or tap where it should go, keeping its size. Delete takes it off the page; Undo puts it back.", style = MaterialTheme.typography.bodySmall)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PointsField("Width (pt)", width, { width = it }, shaper.error != null, Modifier.weight(1f), enabled = shaper.path.bounds.width > 0.0)
                    PointsField("Height (pt)", height, { height = it }, shaper.error != null, Modifier.weight(1f), enabled = shaper.path.bounds.height > 0.0)
                }
                shaper.error?.let { error -> Text(error, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            }
        },
        confirmButton = { Button(onClick = { actions.onResizePath(documentId, width, height) }) { Text("Resize") } },
        dismissButton = {
            // Three actions beside Resize do not fit one row on a phone.
            FlowRow(horizontalArrangement = Arrangement.End) {
                TextButton(onClick = { actions.onDeletePath(documentId) }) { Text("Delete") }
                TextButton(onClick = { actions.onMovePath(documentId) }) { Text("Move") }
                TextButton(onClick = actions.onDismissShaper) { Text("Cancel") }
            }
        },
    )
}
