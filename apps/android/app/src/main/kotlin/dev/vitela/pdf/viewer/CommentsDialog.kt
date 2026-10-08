package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.core.PdfComment

/** Lists saved comments and pending notes without exposing saved entries to editing. */
@Composable
internal fun CommentsDialog(comments: List<PdfComment>, onClose: () -> Unit, onRead: (PdfComment) -> Unit) {
    AlertDialog(
        onDismissRequest = onClose,
        title = { Text("Comments") },
        text = {
            if (comments.isEmpty()) Text("No comments in this document.")
            else LazyColumn(Modifier.heightIn(max = 400.dp)) {
                for ((page, entries) in comments.groupBy { it.pageIndex }.toSortedMap()) {
                    item { Text("Page ${page + 1}") }
                    items(entries) { comment ->
                        TextButton(onClick = { onRead(comment) }, modifier = Modifier.fillMaxWidth()) {
                            Column(Modifier.fillMaxWidth()) {
                                Text(comment.contents, maxLines = 3)
                                val metadata = listOfNotNull(comment.author, comment.date).joinToString(" · ")
                                if (metadata.isNotEmpty()) Text(metadata)
                            }
                        }
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = onClose) { Text("Close") } },
    )
}
