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
import androidx.compose.ui.unit.dp

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
        title = { Text("Resize annotation — page ${resizer.pageIndex + 1}") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("The annotation keeps its bottom-left corner. Width and height are independent; stamps stretch to fit.", style = MaterialTheme.typography.bodySmall)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PointsField("Width (pt)", width, { width = it }, resizer.error != null, Modifier.weight(1f))
                    PointsField("Height (pt)", height, { height = it }, resizer.error != null, Modifier.weight(1f))
                }
                resizer.error?.let { error -> Text(error, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            }
        },
        confirmButton = { Button(onClick = { onResize(documentId, width, height) }) { Text("Resize") } },
        dismissButton = { TextButton(onClick = onCancel) { Text("Cancel") } },
    )
}
