package dev.vitela.pdf.viewer

import androidx.annotation.DrawableRes
import androidx.compose.foundation.layout.size
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R

/** The whole-document actions the old button row carried, now behind the title bar's menu. */
internal data class DocumentMenuActions(
    val onOpen: () -> Unit,
    val onSave: () -> Unit,
    val onSaveCopy: () -> Unit,
    val onPrint: () -> Unit,
    val onProperties: () -> Unit,
    val onExportImages: () -> Unit,
    val onExtractPages: () -> Unit,
    val onSplit: () -> Unit,
    val onCompress: () -> Unit,
    val onProtect: () -> Unit,
    val onAnnotationStep: (forward: Boolean) -> Unit,
)

@Composable
internal fun DocumentMenu(state: ViewerState, expanded: Boolean, onDismiss: () -> Unit, actions: DocumentMenuActions) {
    @Composable
    fun item(label: String, @DrawableRes icon: Int?, enabled: Boolean, onClick: () -> Unit) = DropdownMenuItem(
        text = { Text(label) },
        leadingIcon = icon?.let { { Icon(painterResource(it), contentDescription = null, modifier = Modifier.size(20.dp)) } },
        enabled = enabled,
        onClick = {
            onDismiss()
            onClick()
        },
    )
    val hasPages = state.pageCount > 0
    DropdownMenu(expanded = expanded, onDismissRequest = onDismiss) {
        item("Open PDF", R.drawable.ic_shell_files, state.canOpen, actions.onOpen)
        item("Save", R.drawable.ic_shell_save, state.isDirty && state.saveTarget != null, actions.onSave)
        item("Save copy", null, state.isDirty, actions.onSaveCopy)
        item("Print", R.drawable.ic_shell_print, state.canPrint, actions.onPrint)
        HorizontalDivider()
        item("Compress", R.drawable.ic_shell_compress, hasPages && !state.compressRunning, actions.onCompress)
        item("Protect", R.drawable.ic_shell_protect, hasPages && !state.protectRunning, actions.onProtect)
        item("Export images", R.drawable.ic_shell_export_images, hasPages && !state.imageExportRunning, actions.onExportImages)
        item("Extract pages", R.drawable.ic_shell_extract_pages, hasPages, actions.onExtractPages)
        item("Split", R.drawable.ic_shell_split_pages, state.pageCount > 1 && !state.pageSplitRunning, actions.onSplit)
        HorizontalDivider()
        // Not gated on editing: visiting an annotation changes nothing.
        val annotationNavigation = annotationNavigationEnabled(state)
        item("Previous annotation", R.drawable.ic_shell_previous, annotationNavigation) { actions.onAnnotationStep(false) }
        item("Next annotation", R.drawable.ic_shell_next, annotationNavigation) { actions.onAnnotationStep(true) }
        item("Properties", null, hasPages, actions.onProperties)
    }
}
