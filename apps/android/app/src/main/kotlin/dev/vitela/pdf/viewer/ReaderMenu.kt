package dev.vitela.pdf.viewer

import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.foundation.layout.size
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R

/**
 * The whole-document actions the old button row carried, now behind the
 * title bar's menu, in display order. [startsGroup] draws a divider above.
 */
internal enum class DocumentMenuItem(
    @StringRes val label: Int,
    @DrawableRes val icon: Int?,
    val startsGroup: Boolean = false,
    val enabled: (ViewerState) -> Boolean,
) {
    Open(R.string.reader_menu_open, R.drawable.ic_shell_files, enabled = { it.canOpen }),
    Save(R.string.reader_menu_save, R.drawable.ic_shell_save, enabled = { it.isDirty && it.saveTarget != null }),
    SaveCopy(R.string.reader_menu_save_copy, null, enabled = { it.isDirty }),
    Print(R.string.reader_menu_print, R.drawable.ic_shell_print, enabled = { it.canPrint }),
    Compress(R.string.reader_menu_compress, R.drawable.ic_shell_compress, startsGroup = true, enabled = { it.pageCount > 0 && !it.compressRunning }),
    Protect(R.string.reader_menu_protect, R.drawable.ic_shell_protect, enabled = { it.pageCount > 0 && !it.protectRunning }),
    ExportImages(R.string.reader_menu_export_images, R.drawable.ic_shell_export_images, enabled = { it.pageCount > 0 && !it.imageExportRunning }),
    ExtractPages(R.string.reader_menu_extract_pages, R.drawable.ic_shell_extract_pages, enabled = { it.pageCount > 0 }),
    Split(R.string.reader_menu_split, R.drawable.ic_shell_split_pages, enabled = { it.pageCount > 1 && !it.pageSplitRunning }),

    // Not gated on editing: visiting an annotation changes nothing.
    PreviousAnnotation(R.string.reader_menu_previous_annotation, R.drawable.ic_shell_previous, startsGroup = true, enabled = ::annotationNavigationEnabled),
    NextAnnotation(R.string.reader_menu_next_annotation, R.drawable.ic_shell_next, enabled = ::annotationNavigationEnabled),
    Properties(R.string.reader_menu_properties, null, enabled = { it.pageCount > 0 }),
    Close(R.string.reader_menu_close, null, startsGroup = true, enabled = ::documentCloseEnabled),
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
    val onClose: () -> Unit,
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
        DocumentMenuItem.Close -> onClose()
    }
}

@Composable
internal fun DocumentMenu(state: ViewerState, expanded: Boolean, onDismiss: () -> Unit, actions: DocumentMenuActions) {
    DropdownMenu(expanded = expanded, onDismissRequest = onDismiss) {
        DocumentMenuItem.entries.forEach { item ->
            if (item.startsGroup) HorizontalDivider()
            DropdownMenuItem(
                text = { Text(stringResource(item.label)) },
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
