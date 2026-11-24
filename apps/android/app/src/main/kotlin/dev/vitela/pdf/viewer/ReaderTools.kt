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
import androidx.compose.ui.res.stringResource
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
    val onComments: () -> Unit,
)

@Composable
internal fun QuickTools(state: ViewerState, controls: AnnotationControls, actions: QuickToolActions, sidePanel: Boolean = false) {
    val accent = MaterialTheme.colorScheme.primary
    ToolTray(stringResource(R.string.reader_tools_quick), sidePanel) {
        ToolTile(stringResource(R.string.reader_tool_edit_text), R.drawable.ic_shell_text, accent, actions.onEditText)
        ToolTile(stringResource(R.string.reader_tool_highlight), R.drawable.ic_shell_highlight, ToolHue.Annotate, actions.onHighlight, enabled = controls.canCreate)
        ToolTile(stringResource(R.string.reader_tool_sign), R.drawable.ic_shell_sign, ToolHue.Sign, actions.onSign, enabled = !state.signRunning)
        ToolTile(stringResource(R.string.reader_tool_organize), R.drawable.ic_shell_organize, ToolHue.Organize, actions.onOrganize)
        ToolTile(stringResource(R.string.reader_tool_compress), R.drawable.ic_shell_compress, ToolHue.Compress, actions.onCompress, enabled = !state.compressRunning)
        ToolTile(stringResource(R.string.reader_tool_comments), R.drawable.ic_shell_comments, accent, actions.onComments)
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
    val onWatermark: () -> Unit,
    val contentEdit: ContentEditActions,
)

@Composable
internal fun EditTools(state: ViewerState, controls: AnnotationControls, actions: EditToolActions, sidePanel: Boolean = false) {
    val accent = MaterialTheme.colorScheme.primary
    val armed = state.activeAnnotationTool
    val editing = state.contentEdit != null
    ToolTray(stringResource(if (editing) R.string.reader_tools_edit_content else R.string.reader_tools_edit), sidePanel) {
        ToolTile(stringResource(R.string.reader_tool_undo), R.drawable.ic_shell_undo, accent, actions.onUndo, enabled = controls.canUndo || state.canUndoAnnotations)
        ToolTile(stringResource(R.string.reader_tool_redo), R.drawable.ic_shell_redo, accent, actions.onRedo, enabled = controls.canRedo || state.canRedoAnnotations)
        ToolTile(stringResource(R.string.reader_tool_edit_text), R.drawable.ic_shell_text, accent, actions.contentEdit.onToggle, selected = editing)
        if (editing) {
            ToolTile(stringResource(R.string.reader_tool_add_text), R.drawable.ic_shell_new_file, accent, actions.contentEdit.onAddText, selected = state.contentEdit?.adding != null)
            ToolTile(stringResource(R.string.reader_tool_add_image), R.drawable.ic_shell_image, accent, actions.contentEdit.onAddImage)
            // Named apart from Paste, which stamps the system clipboard's picture: this puts down what Copy or Cut held.
            ToolTile(stringResource(R.string.reader_tool_paste_content), R.drawable.ic_shell_new_file, accent, actions.contentEdit.onPasteContent, enabled = state.contentEdit?.hasClip == true, selected = state.contentEdit?.adding == ContentAddition.Paste)
        }
        for ((tool, label, icon) in ANNOTATION_TOOLS) {
            ToolTile(stringResource(label), icon, ToolHue.Annotate, { actions.onTool(tool) }, enabled = controls.canCreate, selected = armed == tool && tool != AnnotationTool.Pointer)
        }
        ToolTile(stringResource(R.string.reader_tool_stamp), R.drawable.ic_shell_image, ToolHue.Annotate, actions.onChooseStamp, enabled = controls.canCreate, selected = armed == AnnotationTool.Stamp)
        ToolTile(stringResource(R.string.reader_tool_paste), R.drawable.ic_shell_image, ToolHue.Annotate, actions.onPasteStamp, enabled = controls.canCreate)
        ToolTile(stringResource(R.string.reader_tool_organize), R.drawable.ic_shell_organize, ToolHue.Organize, actions.onOrganize)
        // Page content, like Edit text: the permission is asked when it opens.
        ToolTile(stringResource(R.string.reader_tool_watermark), R.drawable.ic_shell_text, accent, actions.onWatermark, enabled = state.pageCount > 0, selected = state.watermark != null)
    }
}

private val ANNOTATION_TOOLS = listOf(
    Triple(AnnotationTool.Pointer, R.string.reader_tool_select, R.drawable.ic_shell_select),
    Triple(AnnotationTool.Highlight, R.string.reader_tool_highlight, R.drawable.ic_shell_highlight),
    Triple(AnnotationTool.Underline, R.string.reader_tool_underline, R.drawable.ic_shell_underline),
    Triple(AnnotationTool.Strikeout, R.string.reader_tool_strikeout, R.drawable.ic_shell_strikeout),
    Triple(AnnotationTool.Ink, R.string.reader_tool_draw, R.drawable.ic_shell_ink),
    Triple(AnnotationTool.Shape, R.string.reader_tool_shape, R.drawable.ic_shell_shape),
    Triple(AnnotationTool.TextNote, R.string.reader_tool_note, R.drawable.ic_shell_comments),
    Triple(AnnotationTool.FreeText, R.string.reader_tool_text_box, R.drawable.ic_shell_text),
)

/** Sign mode: fill the document's form, draw a signature onto a page, or sign it with a certificate. */
@Composable
internal fun SignTools(state: ViewerState, controls: AnnotationControls, onFormFields: () -> Unit, onDrawSignature: () -> Unit, onSign: () -> Unit, sidePanel: Boolean = false) {
    ToolTray(stringResource(R.string.reader_tools_sign), sidePanel) {
        ToolTile(stringResource(R.string.reader_tool_fill_forms), R.drawable.ic_shell_edit, MaterialTheme.colorScheme.primary, onFormFields, selected = state.formFields != null)
        ToolTile(stringResource(R.string.reader_tool_draw_signature), R.drawable.ic_shell_ink, ToolHue.Sign, onDrawSignature, enabled = controls.canCreate, selected = state.activeAnnotationTool == AnnotationTool.Stamp)
        ToolTile(stringResource(R.string.reader_tool_sign_certificate), R.drawable.ic_shell_sign, ToolHue.Sign, onSign, enabled = !state.signRunning)
    }
}
