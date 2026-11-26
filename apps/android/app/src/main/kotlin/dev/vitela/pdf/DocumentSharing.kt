package dev.vitela.pdf

import android.content.Context
import dev.vitela.pdf.document.ShareCopies
import dev.vitela.pdf.document.ShareOutcome
import dev.vitela.pdf.document.ShareSheet
import dev.vitela.pdf.viewer.ViewerViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

/**
 * **Share**: the open document, pending edits included, written to a private
 * cache copy and handed to the system share sheet. The session stays unsaved
 * — a copy sent elsewhere is not this document saved. Every failure lands in
 * the status line; none throws.
 */
internal suspend fun shareDocument(context: Context, viewModel: ViewerViewModel, documentId: Long) {
    val shared = viewModel.shareBytes(documentId) ?: return
    val root = File(context.cacheDir, ShareCopies.ROOT_NAME)
    val copy = withContext(Dispatchers.IO) { ShareCopies.write(root, shared.name, shared.bytes, now = System.currentTimeMillis()) }
    if (copy == null) {
        viewModel.reportShareFailure(shared)
        return
    }
    when (ShareSheet.show(context, copy, copy.name)) {
        ShareOutcome.Shown -> Unit
        ShareOutcome.NoReceiver -> viewModel.reportShareUnavailable(shared)
        ShareOutcome.Failed -> viewModel.reportShareFailure(shared)
    }
}
