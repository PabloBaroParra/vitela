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

/**
 * The whole-document actions the old button row carried, now behind the
 * title bar's menu, in display order. [startsGroup] draws a divider above.
 */
internal enum class DocumentMenuItem(
    val label: String,
    @DrawableRes val icon: Int?,
    val startsGroup: Boolean = false,
    val enabled: (ViewerState) -> Boolean,
) {
    Open("Open PDF", R.drawable.ic_shell_files, enabled = { it.canOpen }),
    Save("Save", R.drawable.ic_shell_save, enabled = { it.isDirty && it.saveTarget != null }),
    SaveCopy("Save copy", null, enabled = { it.isDirty }),
    Print("Print", R.drawable.ic_shell_print, enabled = { it.canPrint }),
    Compress("Compress", R.drawable.ic_shell_compress, startsGroup = true, enabled = { it.pageCount > 0 && !it.compressRunning }),
    Protect("Protect", R.drawable.ic_shell_protect, enabled = { it.pageCount > 0 && !it.protectRunning }),
    ExportImages("Export images", R.drawable.ic_shell_export_images, enabled = { it.pageCount > 0 && !it.imageExportRunning }),
    ExtractPages("Extract pages", R.drawable.ic_shell_extract_pages, enabled = { it.pageCount > 0 }),
    Split("Split", R.drawable.ic_shell_split_pages, enabled = { it.pageCount > 1 && !it.pageSplitRunning }),

    // Not gated on editing: visiting an annotation changes nothing.
    PreviousAnnotation("Previous annotation", R.drawable.ic_shell_previous, startsGroup = true, enabled = ::annotationNavigationEnabled),
    NextAnnotation("Next annotation", R.drawable.ic_shell_next, enabled = ::annotationNavigationEnabled),
    Properties("Properties", null, enabled = { it.pageCount > 0 }),
}

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
) {
    fun run(item: DocumentMenuItem) = when (item) {
        DocumentMenuItem.Open -> onOpen()
        DocumentMenuItem.Save -> onSave()
        DocumentMenuItem.SaveCopy -> onSaveCopy()
        DocumentMenuItem.Print -> onPrint()
        DocumentMenuItem.Compress -> onCompress()
        DocumentMenuItem.Protect -> onProtect()
        DocumentMenuItem.ExportImages -> onExportImages()
        DocumentMenuItem.ExtractPages -> onExtractPages()
        DocumentMenuItem.Split -> onSplit()
        DocumentMenuItem.PreviousAnnotation -> onAnnotationStep(false)
        DocumentMenuItem.NextAnnotation -> onAnnotationStep(true)
        DocumentMenuItem.Properties -> onProperties()
    }
}

@Composable
internal fun DocumentMenu(state: ViewerState, expanded: Boolean, onDismiss: () -> Unit, actions: DocumentMenuActions) {
    DropdownMenu(expanded = expanded, onDismissRequest = onDismiss) {
        DocumentMenuItem.entries.forEach { item ->
            if (item.startsGroup) HorizontalDivider()
            DropdownMenuItem(
                text = { Text(item.label) },
                leadingIcon = item.icon?.let { { Icon(painterResource(it), contentDescription = null, modifier = Modifier.size(20.dp)) } },
                enabled = item.enabled(state),
                onClick = {
                    onDismiss()
                    actions.run(item)
                },
            )
        }
    }
}
