package dev.vitela.pdf.viewer

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.unit.dp

/** What the floating bar over selected text does. */
internal class SelectionActions(
    val onCopy: () -> Unit,
    val onShare: () -> Unit,
    /** Highlight, underline or strike the selection; the selection is spent by it. */
    val onMarkup: (AnnotationTool) -> Unit,
)

/** The selection's colour: its rects, and the handles that drag them. */
internal val SELECTION_COLOR = Color(0xFF3373E6)

/** The knob a finger grabs, and how far from its centre a touch still counts. */
internal const val SELECTION_KNOB_RADIUS_DP = 9
internal const val SELECTION_KNOB_REACH_DP = 28

/**
 * The bar over selected text, like Android's own: Copy and Share always, and
 * the three text markups only when the document allows annotating.
 */
@Composable
internal fun SelectionToolbar(actions: SelectionActions, canMarkup: Boolean, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(8.dp),
        color = MaterialTheme.colorScheme.surface,
        shadowElevation = 6.dp,
        tonalElevation = 2.dp,
    ) {
        Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 4.dp)) {
            TextButton(onClick = actions.onCopy) { Text("Copy") }
            if (canMarkup) {
                TextButton(onClick = { actions.onMarkup(AnnotationTool.Highlight) }) { Text("Highlight") }
                TextButton(onClick = { actions.onMarkup(AnnotationTool.Underline) }) { Text("Underline") }
                TextButton(onClick = { actions.onMarkup(AnnotationTool.Strikeout) }) { Text("Strikeout") }
            }
            TextButton(onClick = actions.onShare) { Text("Share") }
        }
    }
}

/** Each handle: a thin stem along the selection's edge and a round knob hanging under it. */
internal fun DrawScope.drawSelectionHandles(spots: List<HandleSpot>) {
    val radius = SELECTION_KNOB_RADIUS_DP.dp.toPx()
    spots.forEach { spot ->
        drawLine(SELECTION_COLOR, Offset(spot.stemTop.x.toFloat(), spot.stemTop.y.toFloat()), Offset(spot.foot.x.toFloat(), spot.foot.y.toFloat()), 2.dp.toPx())
        drawCircle(SELECTION_COLOR, radius, Offset(spot.knob.x.toFloat(), spot.knob.y.toFloat()))
    }
}
