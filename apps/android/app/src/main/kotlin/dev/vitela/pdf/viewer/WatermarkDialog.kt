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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.core.WatermarkLayer
import dev.vitela.pdf.text.resolve
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
        title = { Text(stringResource(R.string.watermark_title)) },
        text = {
            // Scrolls so the last choice stays reachable above the keyboard.
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.verticalScroll(rememberScrollState())) {
                editor.message?.let { Text(it.resolve(), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
                Text(stringResource(R.string.watermark_kind), style = MaterialTheme.typography.labelLarge)
                Choice(stringResource(R.string.watermark_kind_text), draft.kind == WatermarkKind.Text, enabled = true) { onChange(editor.switchedTo(WatermarkKind.Text)) }
                Choice(stringResource(R.string.watermark_kind_picture), draft.kind == WatermarkKind.Picture, enabled = true) { onChange(editor.switchedTo(WatermarkKind.Picture)) }
                when (draft.kind) {
                    WatermarkKind.Text -> {
                        OutlinedTextField(
                            value = draft.text,
                            onValueChange = { onChange(draft.copy(text = it)) },
                            label = { Text(stringResource(R.string.dialog_text)) },
                            singleLine = true,
                        )
                        LookSlider(stringResource(R.string.watermark_size_points, draft.fontSizePt.roundToInt()), draft.fontSizePt, WATERMARK_FONT_SIZE_RANGE) {
                            onChange(draft.copy(fontSizePt = it))
                        }
                    }
                    WatermarkKind.Picture -> {
                        OutlinedButton(onClick = actions.onChoosePicture) { Text(if (draft.picture == null) stringResource(R.string.watermark_choose_picture) else stringResource(R.string.watermark_choose_another)) }
                        Text(draft.picture?.summary?.resolve() ?: stringResource(R.string.watermark_no_picture), style = MaterialTheme.typography.bodyMedium)
                        LookSlider(stringResource(R.string.watermark_size_percent, draft.scalePercent.roundToInt()), draft.scalePercent, WATERMARK_SCALE_PERCENT_RANGE) {
                            onChange(draft.copy(scalePercent = it))
                        }
                    }
                }
                Text(stringResource(R.string.dialog_pages), style = MaterialTheme.typography.labelLarge)
                Choice(stringResource(R.string.dialog_pages_all), draft.pages == WatermarkPages.All, enabled = true) { onChange(draft.copy(pages = WatermarkPages.All)) }
                Choice(stringResource(R.string.dialog_pages_current), draft.pages == WatermarkPages.Current, enabled = true) { onChange(draft.copy(pages = WatermarkPages.Current)) }
                Choice(stringResource(R.string.dialog_pages_custom), draft.pages == WatermarkPages.Custom, enabled = true) { onChange(draft.copy(pages = WatermarkPages.Custom)) }
                OutlinedTextField(
                    value = draft.range,
                    // Typing a range means a range: no need to pick the choice first.
                    onValueChange = { onChange(draft.copy(pages = WatermarkPages.Custom, range = it)) },
                    label = { Text(stringResource(R.string.dialog_pages_example)) },
                    singleLine = true,
                )
                LookSlider(stringResource(R.string.watermark_opacity, draft.opacityPercent.roundToInt()), draft.opacityPercent, WATERMARK_OPACITY_PERCENT_RANGE) {
                    onChange(draft.copy(opacityPercent = it))
                }
                LookSlider(stringResource(R.string.watermark_angle, draft.rotationDegrees.roundToInt()), draft.rotationDegrees, WATERMARK_ROTATION_RANGE) {
                    onChange(draft.copy(rotationDegrees = it))
                }
                Text(stringResource(R.string.watermark_placement), style = MaterialTheme.typography.labelLarge)
                Choice(stringResource(R.string.watermark_over), draft.layer == WatermarkLayer.OverContent, enabled = true) { onChange(draft.copy(layer = WatermarkLayer.OverContent)) }
                Choice(stringResource(R.string.watermark_behind), draft.layer == WatermarkLayer.UnderContent, enabled = true) { onChange(draft.copy(layer = WatermarkLayer.UnderContent)) }
            }
        },
        confirmButton = { Button(onClick = actions.onAdd) { Text(stringResource(R.string.dialog_add)) } },
        dismissButton = { TextButton(onClick = actions.onDismiss) { Text(stringResource(R.string.dialog_cancel)) } },
    )
}

/** A labelled slider over [range] that only ever reports whole numbers. */
@Composable
private fun LookSlider(label: String, value: Double, range: ClosedFloatingPointRange<Double>, onChange: (Double) -> Unit) {
    Text(label, style = MaterialTheme.typography.labelLarge)
    Slider(
        value = value.toFloat(),
        onValueChange = { onChange(it.roundToInt().toDouble()) },
        valueRange = range.start.toFloat()..range.endInclusive.toFloat(),
    )
}
