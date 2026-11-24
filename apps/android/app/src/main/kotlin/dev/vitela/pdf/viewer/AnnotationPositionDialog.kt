package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
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
 * **Move to**: the selected annotation's bottom-left X and Y in points, ready
 * to change. Keyed on the positioner, so a refusal reopens with what was
 * typed. [documentId] travels with the answer so a dialog left over from a
 * replaced document cannot move anything in the new one.
 */
@Composable
internal fun AnnotationPositionDialog(positioner: AnnotationPositioner, documentId: Long, onMove: (Long, String, String) -> Unit, onCancel: () -> Unit) {
    var x by remember(positioner) { mutableStateOf(positioner.x) }
    var y by remember(positioner) { mutableStateOf(positioner.y) }
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text(stringResource(R.string.annotation_move_title, positioner.annotation.pageIndex + 1)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    stringResource(R.string.annotation_move_help),
                    style = MaterialTheme.typography.bodySmall,
                )
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PointsField("X (pt)", x, { x = it }, positioner.error != null, Modifier.weight(1f))
                    PointsField("Y (pt)", y, { y = it }, positioner.error != null, Modifier.weight(1f))
                }
                positioner.error?.let { error -> Text(error.resolve(), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            }
        },
        confirmButton = { Button(onClick = { onMove(documentId, x, y) }) { Text(stringResource(R.string.dialog_move)) } },
        dismissButton = { TextButton(onClick = onCancel) { Text(stringResource(R.string.dialog_cancel)) } },
    )
}
