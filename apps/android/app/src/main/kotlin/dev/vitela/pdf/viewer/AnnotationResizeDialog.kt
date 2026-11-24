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
 * **Resize**: the selected annotation's width and height in points, ready to
 * change. Keyed on the resizer, so a refusal reopens with what was typed.
 * [documentId] travels with the answer so a dialog left over from a replaced
 * document cannot resize in the new one.
 */
@Composable
internal fun AnnotationResizeDialog(resizer: AnnotationResizer, documentId: Long, onResize: (Long, String, String) -> Unit, onCancel: () -> Unit) {
    var width by remember(resizer) { mutableStateOf(resizer.width) }
    var height by remember(resizer) { mutableStateOf(resizer.height) }
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text(stringResource(R.string.annotation_resize_title, resizer.pageIndex + 1)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.annotation_resize_help), style = MaterialTheme.typography.bodySmall)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PointsField(stringResource(R.string.dialog_width), width, { width = it }, resizer.error != null, Modifier.weight(1f))
                    PointsField(stringResource(R.string.dialog_height), height, { height = it }, resizer.error != null, Modifier.weight(1f))
                }
                resizer.error?.let { error -> Text(error.resolve(), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            }
        },
        confirmButton = { Button(onClick = { onResize(documentId, width, height) }) { Text(stringResource(R.string.dialog_resize)) } },
        dismissButton = { TextButton(onClick = onCancel) { Text(stringResource(R.string.dialog_cancel)) } },
    )
}
