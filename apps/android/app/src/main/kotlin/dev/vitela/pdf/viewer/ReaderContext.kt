package dev.vitela.pdf.viewer

import androidx.annotation.StringRes
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.AssistChip
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.core.AnnotationColor
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.TextStyleToggle

/** What can be done to the current selection — an annotation, a text run being moved. Selected text has its own toolbar. */
internal data class ContextActions(
    val onReadNote: () -> Unit,
    val onEditText: () -> Unit,
    val onGrow: () -> Unit,
    val onResize: () -> Unit,
    val onPosition: () -> Unit,
    val onDelete: () -> Unit,
    val onColor: (AnnotationColor) -> Unit,
    val onCancelMove: () -> Unit,
    val onCancelInsert: () -> Unit,
    val onDeleteSelection: () -> Unit,
    val onClearSelection: () -> Unit,
    val onCopySelection: () -> Unit,
    val onCutSelection: () -> Unit,
    /** Bold, Italic or Underline for the selection's text, one undo step. */
    val onTextStyle: (TextStyleToggle) -> Unit,
) {
    fun run(chip: ContextChip) = when (chip) {
        ContextChip.ReadNote -> onReadNote()
        ContextChip.EditText -> onEditText()
        ContextChip.CancelMove -> onCancelMove()
        ContextChip.CancelInsert -> onCancelInsert()
        ContextChip.Selected -> Unit
        ContextChip.CopySelection -> onCopySelection()
        ContextChip.CutSelection -> onCutSelection()
        ContextChip.DeleteSelection -> onDeleteSelection()
        ContextChip.ClearSelection -> onClearSelection()
        ContextChip.Bold -> onTextStyle(TextStyleToggle.Bold)
        ContextChip.Italic -> onTextStyle(TextStyleToggle.Italic)
        ContextChip.Underline -> onTextStyle(TextStyleToggle.Underline)
        ContextChip.Grow -> onGrow()
        ContextChip.Resize -> onResize()
        ContextChip.MoveTo -> onPosition()
        ContextChip.Red -> onColor(RED_ANNOTATION_COLOR)
        ContextChip.Gold -> onColor(DEFAULT_ANNOTATION_COLOR)
        ContextChip.Delete -> onDelete()
    }
}

/** [label] is null only for the count chip, whose text depends on the selection. */
internal enum class ContextChip(@StringRes val label: Int?) {
    ReadNote(R.string.reader_selection_read_note),
    EditText(R.string.reader_selection_edit_text),
    CancelMove(R.string.reader_selection_cancel_move),
    CancelInsert(R.string.reader_selection_cancel_insert),
    Grow(R.string.reader_selection_grow),
    Resize(R.string.reader_selection_resize),
    MoveTo(R.string.reader_selection_move_to),
    Red(R.string.reader_selection_red),
    Gold(R.string.reader_selection_gold),
    Delete(R.string.reader_selection_delete),

    /** The count of a multi-selection (B25 T-208): a label, not an action. Copy and Cut are its clipboard (T-209). */
    Selected(null),
    CopySelection(R.string.reader_selection_copy),
    CutSelection(R.string.reader_selection_cut),
    DeleteSelection(R.string.reader_selection_delete),
    ClearSelection(R.string.reader_selection_clear),

    /** A selection's text toggles: pressed when its first run has the style. */
    Bold(R.string.content_text_bold),
    Italic(R.string.content_text_italic),
    Underline(R.string.content_text_underline),
}

/** The style a text toggle chip presses, null for every other chip. */
private val ContextChip.textStyle: TextStyleToggle?
    get() = when (this) {
        ContextChip.Bold -> TextStyleToggle.Bold
        ContextChip.Italic -> TextStyleToggle.Italic
        ContextChip.Underline -> TextStyleToggle.Underline
        else -> null
    }

internal fun selectedCount(state: ViewerState): Int = state.contentEdit?.selection?.count ?: 0

/** The text on [this] chip: the count chip reads "N selected", every other its fixed label. */
@Composable
private fun ContextChip.text(state: ViewerState): String {
    if (label != null) return stringResource(label)
    val count = selectedCount(state)
    return pluralStringResource(R.plurals.reader_selection_count, count, count)
}

private val RED_ANNOTATION_COLOR = AnnotationColor(220, 40, 40)

/** The chips for what is selected or armed right now, in display order; empty when nothing is. */
internal fun contextChips(state: ViewerState, controls: AnnotationControls): List<ContextChip> = buildList {
    val selected = state.annotations.lastOrNull { it.id == state.selectedAnnotationId }
    if (readableNote(state) != null) add(ContextChip.ReadNote)
    if (selected?.kind == AnnotationKind.FreeText && state.annotationEditingAllowed) add(ContextChip.EditText)
    if (state.contentEdit?.moving != null) add(ContextChip.CancelMove)
    if (state.contentEdit?.adding != null) add(ContextChip.CancelInsert)
    state.contentEdit?.selection?.let { selection ->
        add(ContextChip.Selected)
        if (selection.blocks.isNotEmpty()) addAll(listOf(ContextChip.Bold, ContextChip.Italic, ContextChip.Underline))
        addAll(listOf(ContextChip.CopySelection, ContextChip.CutSelection, ContextChip.DeleteSelection, ContextChip.ClearSelection))
    }
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
        verticalAlignment = Alignment.CenterVertically,
    ) {
        chips.forEach { chip ->
            val style = chip.textStyle
            // The count is a label, not a button: tapping it would only look like it did something.
            if (style != null) {
                TextStyleChip(style, state.contentEdit?.selectionButtons, actions.onTextStyle)
            } else if (chip == ContextChip.Selected) {
                Text(chip.text(state), style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(horizontal = 4.dp))
            } else {
                AssistChip(onClick = { actions.run(chip) }, label = { Text(chip.text(state)) })
            }
        }
    }
}

/** A small rounded label floating over the pages: the page counter and the zoom level. */
@Composable
private fun Pill(text: String, description: String, enabled: Boolean, onClick: () -> Unit) =
    Pill(description, enabled, onClick) {
        Text(text, style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(horizontal = 14.dp, vertical = 6.dp))
    }

@Composable
private fun IconPill(icon: Int, description: String, enabled: Boolean, onClick: () -> Unit) =
    Pill(description, enabled, onClick) {
        Icon(painterResource(icon), contentDescription = null, modifier = Modifier.padding(horizontal = 8.dp, vertical = 6.dp).size(18.dp))
    }

@Composable
private fun Pill(description: String, enabled: Boolean, onClick: () -> Unit, content: @Composable () -> Unit) {
    Surface(
        onClick = onClick,
        enabled = enabled,
        shape = MaterialTheme.shapes.large,
        color = MaterialTheme.colorScheme.surface.copy(alpha = 0.94f),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        shadowElevation = 2.dp,
        modifier = Modifier.semantics { contentDescription = description },
    ) {
        content()
    }
}

@Composable
internal fun PagePill(state: ViewerState, enabled: Boolean, onClick: () -> Unit) {
    val label = stringResource(R.string.reader_page_of, pageNumber(state), state.pageCount)
    Pill(label, stringResource(R.string.reader_page_pill, label), enabled, onClick)
}

/**
 * Previous page / Next page on either side of the page counter (parity with
 * Windows). Each is disabled at its end of the document rather than wrapping.
 */
@Composable
internal fun PageStepper(state: ViewerState, onPrevious: () -> Unit, onNext: () -> Unit, onPageList: () -> Unit) {
    val enabled = pageNavigationEnabled(state)
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
        IconPill(R.drawable.ic_shell_previous, stringResource(R.string.reader_page_previous), enabled && previousPageTarget(state.pageIndex, state.pageCount) != null, onPrevious)
        PagePill(state, enabled, onPageList)
        IconPill(R.drawable.ic_shell_next, stringResource(R.string.reader_page_next), enabled && nextPageTarget(state.pageIndex, state.pageCount) != null, onNext)
    }
}

@Composable
internal fun ZoomPill(state: ViewerState, zoom: ZoomActions) {
    var open by remember { mutableStateOf(false) }
    val percentage = (state.zoomFactor * 100).toInt()
    Box {
        Pill("$percentage%", stringResource(R.string.reader_zoom_level, percentage), enabled = state.pageCount > 0) { open = true }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            @Composable
            fun item(label: String, enabled: Boolean, onClick: () -> Unit) =
                DropdownMenuItem(text = { Text(label) }, enabled = enabled, onClick = { open = false; onClick() })
            item(stringResource(R.string.reader_zoom_in), state.zoomFactor < MAX_ZOOM_FACTOR, zoom.onZoomIn)
            item(stringResource(R.string.reader_zoom_out), state.zoomFactor > MIN_ZOOM_FACTOR, zoom.onZoomOut)
            item(stringResource(R.string.reader_zoom_fit_width), state.zoomFactor != DEFAULT_ZOOM_FACTOR, zoom.onFitWidth)
            item(stringResource(R.string.reader_zoom_fit_page), true, zoom.onFitPage)
        }
    }
}
