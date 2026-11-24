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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.text.resolve

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
        title = { Text(stringResource(R.string.content_shape_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.content_shape_help), style = MaterialTheme.typography.bodySmall)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PointsField(stringResource(R.string.dialog_width), width, { width = it }, shaper.error != null, Modifier.weight(1f), enabled = shaper.path.bounds.width > 0.0)
                    PointsField(stringResource(R.string.dialog_height), height, { height = it }, shaper.error != null, Modifier.weight(1f), enabled = shaper.path.bounds.height > 0.0)
                }
                shaper.error?.let { error -> Text(error.resolve(), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            }
        },
        confirmButton = { Button(onClick = { actions.onResizePath(documentId, width, height) }) { Text(stringResource(R.string.dialog_resize)) } },
        dismissButton = {
            // Three actions beside Resize do not fit one row on a phone.
            FlowRow(horizontalArrangement = Arrangement.End) {
                TextButton(onClick = { actions.onDeletePath(documentId) }) { Text(stringResource(R.string.dialog_delete)) }
                TextButton(onClick = { actions.onMovePath(documentId) }) { Text(stringResource(R.string.dialog_move)) }
                TextButton(onClick = actions.onDismissShaper) { Text(stringResource(R.string.dialog_cancel)) }
            }
        },
    )
}
