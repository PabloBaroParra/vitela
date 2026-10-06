package dev.vitela.pdf.viewer

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Checkbox
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp

/** What the Sign tab and the pad hand back; the activity turns strokes into a PNG off the main thread. */
internal data class DrawnSignatureActions(
    val onOpen: () -> Unit,
    val onUse: (documentId: Long, strokes: List<List<PadPoint>>, strokeWidth: Float, remember: Boolean) -> Unit,
    val onCancel: () -> Unit,
    val saved: SavedSignatureActions,
)

/** Pen width on the pad; the PNG keeps it, so the signature looks as drawn. */
private val SIGNATURE_STROKE = 3.dp

/**
 * **Draw signature**: a white pad the user signs with a finger. **Use**
 * stays disabled until a line exists and hands the strokes, with the pen
 * width in the same pad pixels, to [onUse]. [documentId] travels with the
 * answer so a pad left over from a replaced document cannot arm on the new one.
 * **Remember** — ticked by default — keeps the signature on this phone for the
 * next time; untick it to sign once.
 *
 * The pad is white whatever the theme — ink is always black, as on paper.
 */
@Composable
internal fun SignaturePadDialog(documentId: Long, onUse: (Long, List<List<PadPoint>>, Float, Boolean) -> Unit, onCancel: () -> Unit) {
    var remember by remember { mutableStateOf(true) }
    val strokes = remember { mutableStateListOf<List<PadPoint>>() }
    val current = remember { mutableStateListOf<PadPoint>() }
    val strokeWidth = with(LocalDensity.current) { SIGNATURE_STROKE.toPx() }
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text("Draw your signature") },
        text = {
            Column {
                Canvas(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(200.dp)
                        .background(Color.White, MaterialTheme.shapes.small)
                        .border(1.dp, MaterialTheme.colorScheme.outline, MaterialTheme.shapes.small)
                        .pointerInput(Unit) {
                            detectDragGestures(
                                onDragStart = { current.clear(); current.add(it.toPadPoint()) },
                                onDrag = { change, _ -> current.add(change.position.toPadPoint()) },
                                onDragEnd = { if (current.size >= 2) strokes.add(current.toList()); current.clear() },
                                onDragCancel = { current.clear() },
                            )
                        },
                ) {
                    val pen = Stroke(width = strokeWidth, cap = StrokeCap.Round, join = StrokeJoin.Round)
                    for (stroke in strokes + listOf(current.toList())) {
                        if (stroke.size < 2) continue
                        val path = Path().apply {
                            moveTo(stroke[0].x, stroke[0].y)
                            for (point in stroke.drop(1)) lineTo(point.x, point.y)
                        }
                        drawPath(path, Color.Black, style = pen)
                    }
                }
                Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Checkbox(checked = remember, onCheckedChange = { remember = it })
                    Text("Remember on this phone")
                }
            }
        },
        confirmButton = {
            Button(onClick = { onUse(documentId, strokes.toList(), strokeWidth, remember) }, enabled = hasSignatureInk(strokes)) { Text("Use") }
        },
        dismissButton = {
            Row {
                TextButton(onClick = { strokes.clear() }, enabled = strokes.isNotEmpty()) { Text("Clear") }
                TextButton(onClick = onCancel) { Text("Cancel") }
            }
        },
    )
}

private fun Offset.toPadPoint() = PadPoint(x, y)
