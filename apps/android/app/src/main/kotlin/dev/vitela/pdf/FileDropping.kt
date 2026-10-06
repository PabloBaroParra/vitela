package dev.vitela.pdf

import android.app.Activity
import android.content.ContentResolver
import android.content.Context
import android.content.ContextWrapper
import android.net.Uri
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.draganddrop.DragAndDropEvent
import androidx.compose.ui.draganddrop.toAndroidDragEvent
import androidx.compose.ui.platform.LocalContext
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.document.SafDocuments
import dev.vitela.pdf.viewer.DROP_IMAGE_OFF_PAGE
import dev.vitela.pdf.viewer.DROP_IMAGE_WITHOUT_DOCUMENT
import dev.vitela.pdf.viewer.DROP_UNREADABLE
import dev.vitela.pdf.viewer.DROP_UNSUPPORTED
import dev.vitela.pdf.viewer.DroppedFile
import dev.vitela.pdf.viewer.FileDropActions
import dev.vitela.pdf.viewer.ViewerViewModel
import dev.vitela.pdf.viewer.droppedFile
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Files dragged in from another app — the other half of a split screen, a
 * freeform window (T-092). A PDF opens through the same unsaved-changes guard
 * as the picker; an image dropped on a page becomes a stamp at the drop point.
 *
 * The URIs arrive with a grant that lasts only as long as the permissions this
 * asks for at the drop, so the file is read once and the grant handed back.
 * That grant is never persistable, so **Save** on a dropped PDF falls back to
 * Save a copy — the same as a document handed over by "Open with".
 */
@Composable
internal fun rememberFileDropActions(viewModel: ViewerViewModel, scope: CoroutineScope): FileDropActions {
    val context = LocalContext.current
    return remember(viewModel, scope, context) {
        val drops = FileDrops(context, viewModel, scope)
        FileDropActions(
            onPage = { pageIndex, point, event -> drops.drop(event, pageIndex to point) },
            onScreen = { event -> drops.drop(event, page = null) },
        )
    }
}

private class FileDrops(
    private val context: Context,
    private val viewModel: ViewerViewModel,
    private val scope: CoroutineScope,
) {
    private val resolver: ContentResolver = context.contentResolver

    fun drop(event: DragAndDropEvent, page: Pair<Int, AnnotationPoint>?): Boolean {
        val drag = event.toAndroidDragEvent()
        val clip = drag.clipData ?: return false
        // Must be asked for while the drop is being delivered; the grant is gone once it returns.
        val permissions = context.findActivity()?.requestDragAndDropPermissions(drag)
        val uris = (0 until clip.itemCount).map { clip.getItemAt(it).uri?.toString() }
        val dropped = droppedFile(uris) { runCatching { resolver.getType(Uri.parse(it)) }.getOrNull() }
        val documentId = viewModel.state.value.documentId
        val release = { permissions?.release() }
        when {
            dropped == null || dropped == DroppedFile.Unsupported -> {
                release()
                viewModel.refuseDrop(DROP_UNSUPPORTED)
            }
            dropped is DroppedFile.Pdf -> scope.launch {
                val opened = try {
                    withContext(Dispatchers.IO) { SafDocuments.open(resolver, Uri.parse(dropped.uri)) }
                } finally {
                    release()
                }
                if (opened == null) viewModel.refuseDrop(DROP_UNREADABLE) else viewModel.open(opened.displayName, opened.bytes, saveTarget = opened.saveTarget)
            }
            dropped is DroppedFile.Image && page == null -> {
                release()
                viewModel.refuseDrop(if (documentId == 0L) DROP_IMAGE_WITHOUT_DOCUMENT else DROP_IMAGE_OFF_PAGE)
            }
            dropped is DroppedFile.Image && page != null -> scope.launch {
                val bytes = try {
                    withContext(Dispatchers.IO) { runCatching { resolver.openInputStream(Uri.parse(dropped.uri))?.use { it.readBytes() } }.getOrNull() }
                } finally {
                    release()
                }
                // A different document opened while the file was being read: the drop was aimed at the old one.
                if (viewModel.state.value.documentId != documentId) return@launch
                if (bytes == null) viewModel.refuseDrop(DROP_UNREADABLE) else viewModel.dropImageStamp(page.first, page.second, bytes)
            }
        }
        return true
    }
}

private tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}
