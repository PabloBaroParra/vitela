package dev.vitela.pdf.viewer

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.AssistChip
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.core.AnnotationColor

/** What can be done to the current selection — an annotation, a text run being moved, copied text. */
internal data class ContextActions(
    val onCopy: () -> Unit,
    val onReadNote: () -> Unit,
    val onGrow: () -> Unit,
    val onResize: () -> Unit,
    val onPosition: () -> Unit,
    val onDelete: () -> Unit,
    val onColor: (AnnotationColor) -> Unit,
    val onCancelMove: () -> Unit,
    val onCancelInsert: () -> Unit,
)

/**
 * A chip row that exists only while something is selected or armed, so the
 * reader is not permanently paying for controls that do nothing.
 */
@Composable
internal fun ContextChips(state: ViewerState, controls: AnnotationControls, actions: ContextActions) {
    val selected = state.annotations.lastOrNull { it.id == state.selectedAnnotationId }
    val chips = buildList<Pair<String, () -> Unit>> {
        if (state.textSelection != null) add("Copy" to actions.onCopy)
        if (readableNote(state) != null) add("Read note" to actions.onReadNote)
        if (state.contentEdit?.moving != null) add("Cancel move" to actions.onCancelMove)
        if (state.contentEdit?.adding != null) add("Cancel insert" to actions.onCancelInsert)
        if (controls.canGrow) add("Grow" to actions.onGrow)
        if (controls.canResize) add("Resize" to actions.onResize)
        if (controls.canMove) add("Move to" to actions.onPosition)
        if (controls.canRestyle) {
            add("Red" to { actions.onColor(AnnotationColor(220, 40, 40)) })
            add("Gold" to { actions.onColor(DEFAULT_ANNOTATION_COLOR) })
        }
        if (selected != null && state.annotationEditingAllowed) add("Delete" to actions.onDelete)
    }
    if (chips.isEmpty()) return
    Row(
        modifier = Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        chips.forEach { (label, onClick) -> AssistChip(onClick = onClick, label = { Text(label) }) }
    }
}

/** A small rounded label floating over the pages: the page counter and the zoom level. */
@Composable
private fun Pill(text: String, description: String, enabled: Boolean, onClick: () -> Unit) {
    Surface(
        onClick = onClick,
        enabled = enabled,
        shape = MaterialTheme.shapes.large,
        color = MaterialTheme.colorScheme.surface.copy(alpha = 0.94f),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        shadowElevation = 2.dp,
        modifier = Modifier.semantics { contentDescription = description },
    ) {
        Text(text, style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(horizontal = 14.dp, vertical = 6.dp))
    }
}

@Composable
internal fun PagePill(state: ViewerState, enabled: Boolean, onClick: () -> Unit) =
    Pill(pageLabel(state), "Page ${pageLabel(state)}. Go to page", enabled, onClick)

@Composable
internal fun ZoomPill(state: ViewerState, zoom: ZoomActions) {
    var open by remember { mutableStateOf(false) }
    val percentage = (state.zoomFactor * 100).toInt()
    Box {
        Pill("$percentage%", "Zoom level: $percentage%", enabled = state.pageCount > 0) { open = true }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            @Composable
            fun item(label: String, enabled: Boolean, onClick: () -> Unit) =
                DropdownMenuItem(text = { Text(label) }, enabled = enabled, onClick = { open = false; onClick() })
            item("Zoom in", state.zoomFactor < MAX_ZOOM_FACTOR, zoom.onZoomIn)
            item("Zoom out", state.zoomFactor > MIN_ZOOM_FACTOR, zoom.onZoomOut)
            item("Fit width", state.zoomFactor != DEFAULT_ZOOM_FACTOR, zoom.onFitWidth)
            item("Fit page", true, zoom.onFitPage)
        }
    }
}
