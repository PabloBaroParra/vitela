package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import dev.vitela.pdf.core.CompressPreset
import dev.vitela.pdf.core.DocumentInfo

/** The document-level dialogs, each shown while its editor is in [ViewerState]. */
internal data class DialogActions(
    val onPassword: (String) -> Unit,
    val onPasswordCancel: () -> Unit,
    val onReplacementConfirmed: () -> Unit,
    val onReplacementCancelled: () -> Unit,
    val onNoteAdd: (Long, String) -> Unit,
    val onNoteCancel: () -> Unit,
    val onNoteReadingClose: () -> Unit,
    val onAnnotationResizeConfirm: (Long, String, String) -> Unit,
    val onAnnotationResizeCancel: () -> Unit,
    val onAnnotationPositionConfirm: (Long, String, String) -> Unit,
    val onAnnotationPositionCancel: () -> Unit,
    val onMetadataChange: (DocumentInfo) -> Unit,
    val onMetadataApply: () -> Unit,
    val onMetadataDismiss: () -> Unit,
    val onImageExportChange: (ImageExportDraft) -> Unit,
    val onImageExportConfirm: () -> Unit,
    val onImageExportDismiss: () -> Unit,
    val onPageExtractChange: (String) -> Unit,
    val onPageExtractConfirm: () -> Unit,
    val onPageExtractDismiss: () -> Unit,
    val onPageSplitChange: (String) -> Unit,
    val onPageSplitConfirm: () -> Unit,
    val onPageSplitDismiss: () -> Unit,
    val onCompressSelect: (CompressPreset) -> Unit,
    val onCompressConfirm: () -> Unit,
    val onCompressDismiss: () -> Unit,
    val onProtectConfirm: (openPassword: String, permissionsPassword: String) -> Unit,
    val onProtectDismiss: () -> Unit,
    val sign: SignActions,
    val contentEdit: ContentEditActions,
)

@Composable
internal fun ViewerDialogs(state: ViewerState, actions: DialogActions) {
    if (state.needsPassword) PasswordDialog(state.passwordMessage, actions.onPassword, actions.onPasswordCancel)
    state.contentEdit?.editor?.let { TextRunEditorDialog(it, state.documentId, actions.contentEdit) }
    state.contentEdit?.resizer?.let { ImageResizerDialog(it, state.documentId, actions.contentEdit) }
    state.contentEdit?.inserter?.let { TextInserterDialog(it, state.documentId, actions.contentEdit) }
    state.notePlacement?.let { NoteDialog(it, state.documentId, onAdd = actions.onNoteAdd, onCancel = actions.onNoteCancel) }
    state.noteReading?.let { NoteReadingDialog(it, onClose = actions.onNoteReadingClose) }
    state.annotationResizer?.let {
        AnnotationResizeDialog(it, state.documentId, onResize = actions.onAnnotationResizeConfirm, onCancel = actions.onAnnotationResizeCancel)
    }
    state.annotationPositioner?.let {
        AnnotationPositionDialog(it, state.documentId, onMove = actions.onAnnotationPositionConfirm, onCancel = actions.onAnnotationPositionCancel)
    }
    state.metadataEditor?.let { MetadataDialog(it, onChange = actions.onMetadataChange, onApply = actions.onMetadataApply, onDismiss = actions.onMetadataDismiss) }
    state.imageExport?.let {
        ImageExportDialog(it, onChange = actions.onImageExportChange, onExport = actions.onImageExportConfirm, onDismiss = actions.onImageExportDismiss)
    }
    state.pageExtract?.let {
        PageExtractDialog(it, onChange = actions.onPageExtractChange, onExtract = actions.onPageExtractConfirm, onDismiss = actions.onPageExtractDismiss)
    }
    state.pageSplit?.let {
        PageSplitDialog(it, state.pageCount, onChange = actions.onPageSplitChange, onSplit = actions.onPageSplitConfirm, onDismiss = actions.onPageSplitDismiss)
    }
    state.compress?.let { CompressDialog(it, onSelect = actions.onCompressSelect, onCompress = actions.onCompressConfirm, onDismiss = actions.onCompressDismiss) }
    state.protect?.let { ProtectDialog(it, onProtect = actions.onProtectConfirm, onDismiss = actions.onProtectDismiss) }
    state.sign?.let { SignDialog(it, actions.sign) }
    state.pendingReplacementTitle?.let { title ->
        AlertDialog(
            onDismissRequest = actions.onReplacementCancelled,
            title = { Text("Discard unsaved changes?") },
            text = { Text("Open $title and discard the current unsaved changes?") },
            confirmButton = { Button(onClick = actions.onReplacementConfirmed) { Text("Discard and open") } },
            dismissButton = { TextButton(onClick = actions.onReplacementCancelled) { Text("Keep editing") } },
        )
    }
}

@Composable
private fun PasswordDialog(message: String?, onPassword: (String) -> Unit, onCancel: () -> Unit) {
    var password by remember { mutableStateOf("") }
    var passwordVisible by remember { mutableStateOf(false) }
    val cancel = { onCancel(); password = "" }
    AlertDialog(
        onDismissRequest = cancel,
        title = { Text("Password required") },
        text = {
            Column {
                message?.let { Text(it) }
                OutlinedTextField(
                    value = password,
                    onValueChange = { password = it },
                    label = { Text("Password") },
                    visualTransformation = if (passwordVisible) VisualTransformation.None else PasswordVisualTransformation(),
                    trailingIcon = {
                        TextButton(onClick = { passwordVisible = !passwordVisible }) {
                            Text(if (passwordVisible) "Hide" else "Show")
                        }
                    },
                )
            }
        },
        confirmButton = { Button(onClick = { onPassword(password); password = "" }) { Text("Open") } },
        dismissButton = { TextButton(onClick = cancel) { Text("Cancel") } },
    )
}
