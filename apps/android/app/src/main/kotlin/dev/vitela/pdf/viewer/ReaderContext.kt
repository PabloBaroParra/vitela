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
) {
    fun run(chip: ContextChip) = when (chip) {
        ContextChip.Copy -> onCopy()
        ContextChip.ReadNote -> onReadNote()
        ContextChip.CancelMove -> onCancelMove()
        ContextChip.CancelInsert -> onCancelInsert()
        ContextChip.Grow -> onGrow()
        ContextChip.Resize -> onResize()
        ContextChip.MoveTo -> onPosition()
        ContextChip.Red -> onColor(RED_ANNOTATION_COLOR)
        ContextChip.Gold -> onColor(DEFAULT_ANNOTATION_COLOR)
        ContextChip.Delete -> onDelete()
    }
}

internal enum class ContextChip(val label: String) {
    Copy("Copy"), ReadNote("Read note"), CancelMove("Cancel move"), CancelInsert("Cancel insert"),
    Grow("Grow"), Resize("Resize"), MoveTo("Move to"), Red("Red"), Gold("Gold"), Delete("Delete"),
}

private val RED_ANNOTATION_COLOR = AnnotationColor(220, 40, 40)

/** The chips for what is selected or armed right now, in display order; empty when nothing is. */
internal fun contextChips(state: ViewerState, controls: AnnotationControls): List<ContextChip> = buildList {
    val selected = state.annotations.lastOrNull { it.id == state.selectedAnnotationId }
    if (state.textSelection != null) add(ContextChip.Copy)
    if (readableNote(state) != null) add(ContextChip.ReadNote)
    if (state.contentEdit?.moving != null) add(ContextChip.CancelMove)
    if (state.contentEdit?.adding != null) add(ContextChip.CancelInsert)
    if (controls.canGrow) add(ContextChip.Grow)
    if (controls.canResize) add(ContextChip.Resize)
    if (controls.canMove) add(ContextChip.MoveTo)
    if (controls.canRestyle) addAll(listOf(ContextChip.Red, ContextChip.Gold))
    if (selected != null && state.annotationEditingAllowed) add(ContextChip.Delete)
}

/**
 * A chip row that exists only while something is selected or armed, so the
 * reader is not permanently paying for controls that do nothing.
 */
@Composable
internal fun ContextChips(state: ViewerState, controls: AnnotationControls, actions: ContextActions) {
    val chips = contextChips(state, controls)
    if (chips.isEmpty()) return
    Row(
        modifier = Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        chips.forEach { chip -> AssistChip(onClick = { actions.run(chip) }, label = { Text(chip.label) }) }
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
