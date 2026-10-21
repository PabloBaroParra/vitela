package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.core.WatermarkLayer
import kotlin.math.roundToInt

/** The Add watermark dialog's callbacks, held by the screen as one group. */
internal class WatermarkActions(
    val onOpen: () -> Unit,
    val onChange: (WatermarkDraft) -> Unit,
    val onChoosePicture: () -> Unit,
    val onAdd: () -> Unit,
    val onDismiss: () -> Unit,
)

/**
 * Add watermark: text or a picture, which pages, its size, opacity and angle,
 * and whether it goes over the page content or behind it. It stays open on a
 * refused Add and says why under the title, so a bad range, a character the
 * font cannot show or a file that is not a picture is fixed in place.
 *
 * Sliders rather than number boxes: on a phone they are one thumb, and their
 * ranges are the limits — nothing typed can be out of bounds.
 */
@Composable
internal fun WatermarkDialog(editor: WatermarkEditor, actions: WatermarkActions) {
    val draft = editor.draft
    val onChange = actions.onChange
    AlertDialog(
        onDismissRequest = actions.onDismiss,
        title = { Text("Add watermark") },
        text = {
            // Scrolls so the last choice stays reachable above the keyboard.
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.verticalScroll(rememberScrollState())) {
                editor.message?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
                Text("Watermark", style = MaterialTheme.typography.labelLarge)
                Choice("Text", draft.kind == WatermarkKind.Text, enabled = true) { onChange(editor.switchedTo(WatermarkKind.Text)) }
                Choice("Picture", draft.kind == WatermarkKind.Picture, enabled = true) { onChange(editor.switchedTo(WatermarkKind.Picture)) }
                when (draft.kind) {
                    WatermarkKind.Text -> {
                        OutlinedTextField(
                            value = draft.text,
                            onValueChange = { onChange(draft.copy(text = it)) },
                            label = { Text("Text") },
                            singleLine = true,
                        )
                        LookSlider("Size", "${draft.fontSizePt.roundToInt()} pt", draft.fontSizePt, WATERMARK_FONT_SIZE_RANGE) {
                            onChange(draft.copy(fontSizePt = it))
                        }
                    }
                    WatermarkKind.Picture -> {
                        OutlinedButton(onClick = actions.onChoosePicture) { Text(if (draft.picture == null) "Choose picture…" else "Choose another picture…") }
                        Text(draft.picture?.summary ?: "No picture chosen. PNG or JPEG.", style = MaterialTheme.typography.bodyMedium)
                        LookSlider("Size", "${draft.scalePercent.roundToInt()}% of the page", draft.scalePercent, WATERMARK_SCALE_PERCENT_RANGE) {
                            onChange(draft.copy(scalePercent = it))
                        }
                    }
                }
                Text("Pages", style = MaterialTheme.typography.labelLarge)
                Choice("All pages", draft.pages == WatermarkPages.All, enabled = true) { onChange(draft.copy(pages = WatermarkPages.All)) }
                Choice("Current page", draft.pages == WatermarkPages.Current, enabled = true) { onChange(draft.copy(pages = WatermarkPages.Current)) }
                Choice("Custom range", draft.pages == WatermarkPages.Custom, enabled = true) { onChange(draft.copy(pages = WatermarkPages.Custom)) }
                OutlinedTextField(
                    value = draft.range,
                    // Typing a range means a range: no need to pick the choice first.
                    onValueChange = { onChange(draft.copy(pages = WatermarkPages.Custom, range = it)) },
                    label = { Text("e.g. 1-3,7") },
                    singleLine = true,
                )
                LookSlider("Opacity", "${draft.opacityPercent.roundToInt()}%", draft.opacityPercent, WATERMARK_OPACITY_PERCENT_RANGE) {
                    onChange(draft.copy(opacityPercent = it))
                }
                LookSlider("Angle", "${draft.rotationDegrees.roundToInt()}°", draft.rotationDegrees, WATERMARK_ROTATION_RANGE) {
                    onChange(draft.copy(rotationDegrees = it))
                }
                Text("Placement", style = MaterialTheme.typography.labelLarge)
                Choice("Over the page content", draft.layer == WatermarkLayer.OverContent, enabled = true) { onChange(draft.copy(layer = WatermarkLayer.OverContent)) }
                Choice("Behind the page content", draft.layer == WatermarkLayer.UnderContent, enabled = true) { onChange(draft.copy(layer = WatermarkLayer.UnderContent)) }
            }
        },
        confirmButton = { Button(onClick = actions.onAdd) { Text("Add") } },
        dismissButton = { TextButton(onClick = actions.onDismiss) { Text("Cancel") } },
    )
}

/** A labelled slider over [range] that only ever reports whole numbers. */
@Composable
private fun LookSlider(label: String, shown: String, value: Double, range: ClosedFloatingPointRange<Double>, onChange: (Double) -> Unit) {
    Text("$label: $shown", style = MaterialTheme.typography.labelLarge)
    Slider(
        value = value.toFloat(),
        onValueChange = { onChange(it.roundToInt().toDouble()) },
        valueRange = range.start.toFloat()..range.endInclusive.toFloat(),
    )
}
