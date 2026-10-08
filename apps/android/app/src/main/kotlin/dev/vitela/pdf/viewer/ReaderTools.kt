package dev.vitela.pdf.viewer

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.ui.components.ToolTile
import dev.vitela.pdf.ui.theme.ToolHue

/** The bottom sheet-like card every mode's tools sit in, edge to edge above the navigation bar. */
@Composable
@OptIn(ExperimentalLayoutApi::class)
private fun ToolTray(title: String, sidePanel: Boolean, content: @Composable () -> Unit) {
    Surface(
        color = MaterialTheme.colorScheme.surface,
        shape = MaterialTheme.shapes.large.copy(bottomStart = MaterialTheme.shapes.small.bottomStart, bottomEnd = MaterialTheme.shapes.small.bottomEnd),
        shadowElevation = 6.dp,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(modifier = Modifier.navigationBarsPadding().padding(top = 14.dp, bottom = 10.dp)) {
            Text(title, style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(horizontal = 16.dp))
            if (sidePanel) FlowRow(
                modifier = Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) { content() }
            else Row(
                modifier = Modifier.horizontalScroll(rememberScrollState()).padding(PaddingValues(horizontal = 12.dp, vertical = 10.dp)),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) { content() }
        }
    }
}

/** Read mode's shortcuts: each one switches to the mode that owns the tool, then arms it. */
internal data class QuickToolActions(
    val onEditText: () -> Unit,
    val onHighlight: () -> Unit,
    val onSign: () -> Unit,
    val onOrganize: () -> Unit,
    val onCompress: () -> Unit,
)

@Composable
internal fun QuickTools(state: ViewerState, controls: AnnotationControls, actions: QuickToolActions, sidePanel: Boolean = false) {
    val accent = MaterialTheme.colorScheme.primary
    ToolTray("Quick tools", sidePanel) {
        ToolTile("Edit text", R.drawable.ic_shell_text, accent, actions.onEditText)
        ToolTile("Highlight", R.drawable.ic_shell_highlight, ToolHue.Annotate, actions.onHighlight, enabled = controls.canCreate)
        ToolTile("Sign", R.drawable.ic_shell_sign, ToolHue.Sign, actions.onSign, enabled = !state.signRunning)
        ToolTile("Organize pages", R.drawable.ic_shell_organize, ToolHue.Organize, actions.onOrganize)
        ToolTile("Compress", R.drawable.ic_shell_compress, ToolHue.Compress, actions.onCompress, enabled = !state.compressRunning)
    }
}

/** Edit mode: the annotation tools, then content editing, then page-level history. */
internal data class EditToolActions(
    val onTool: (AnnotationTool) -> Unit,
    val onChooseStamp: () -> Unit,
    val onPasteStamp: () -> Unit,
    val onUndo: () -> Unit,
    val onRedo: () -> Unit,
    val onOrganize: () -> Unit,
    val contentEdit: ContentEditActions,
)

@Composable
internal fun EditTools(state: ViewerState, controls: AnnotationControls, actions: EditToolActions, sidePanel: Boolean = false) {
    val accent = MaterialTheme.colorScheme.primary
    val armed = state.activeAnnotationTool
    val editing = state.contentEdit != null
    ToolTray(if (editing) "Edit content" else "Edit", sidePanel) {
        ToolTile("Undo", R.drawable.ic_shell_undo, accent, actions.onUndo, enabled = controls.canUndo || state.canUndoAnnotations)
        ToolTile("Redo", R.drawable.ic_shell_redo, accent, actions.onRedo, enabled = controls.canRedo || state.canRedoAnnotations)
        ToolTile("Edit text", R.drawable.ic_shell_text, accent, actions.contentEdit.onToggle, selected = editing)
        if (editing) {
            ToolTile("Add text", R.drawable.ic_shell_new_file, accent, actions.contentEdit.onAddText, selected = state.contentEdit?.adding != null)
            ToolTile("Add image", R.drawable.ic_shell_image, accent, actions.contentEdit.onAddImage)
        }
        for ((tool, label, icon) in ANNOTATION_TOOLS) {
            ToolTile(label, icon, ToolHue.Annotate, { actions.onTool(tool) }, enabled = controls.canCreate, selected = armed == tool && tool != AnnotationTool.Pointer)
        }
        ToolTile("Stamp", R.drawable.ic_shell_image, ToolHue.Annotate, actions.onChooseStamp, enabled = controls.canCreate, selected = armed == AnnotationTool.Stamp)
        ToolTile("Paste", R.drawable.ic_shell_image, ToolHue.Annotate, actions.onPasteStamp, enabled = controls.canCreate)
        ToolTile("Organize pages", R.drawable.ic_shell_organize, ToolHue.Organize, actions.onOrganize)
    }
}

private val ANNOTATION_TOOLS = listOf(
    Triple(AnnotationTool.Pointer, "Select", R.drawable.ic_shell_select),
    Triple(AnnotationTool.Highlight, "Highlight", R.drawable.ic_shell_highlight),
    Triple(AnnotationTool.Underline, "Underline", R.drawable.ic_shell_underline),
    Triple(AnnotationTool.Strikeout, "Strikeout", R.drawable.ic_shell_strikeout),
    Triple(AnnotationTool.Ink, "Draw", R.drawable.ic_shell_ink),
    Triple(AnnotationTool.Shape, "Shape", R.drawable.ic_shell_shape),
    Triple(AnnotationTool.TextNote, "Note", R.drawable.ic_shell_comments),
    Triple(AnnotationTool.FreeText, "Text box", R.drawable.ic_shell_text),
)

/** Sign mode: fill the document's form, draw a signature onto a page, or sign it with a certificate. */
@Composable
internal fun SignTools(state: ViewerState, controls: AnnotationControls, onFormFields: () -> Unit, onDrawSignature: () -> Unit, onSign: () -> Unit, sidePanel: Boolean = false) {
    ToolTray("Fill & sign", sidePanel) {
        ToolTile("Fill forms", R.drawable.ic_shell_edit, MaterialTheme.colorScheme.primary, onFormFields, selected = state.formFields != null)
        ToolTile("Draw signature", R.drawable.ic_shell_ink, ToolHue.Sign, onDrawSignature, enabled = controls.canCreate, selected = state.activeAnnotationTool == AnnotationTool.Stamp)
        ToolTile("Sign with certificate", R.drawable.ic_shell_sign, ToolHue.Sign, onSign, enabled = !state.signRunning)
    }
}
