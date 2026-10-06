package dev.vitela.pdf.viewer

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp

/** The offer's answers; each names nothing the ViewModel does not already hold. */
internal data class SavedSignatureActions(
    val onUse: (documentId: Long) -> Unit,
    val onDrawNew: () -> Unit,
    val onDelete: () -> Unit,
    val onCancel: () -> Unit,
)

/**
 * The signature remembered on this phone, offered before a blank pad: **Use**
 * places it as it is, **Draw new** opens the pad, **Delete** forgets it here.
 * Shown on white, as the pad is — the ink is black.
 */
@Composable
internal fun SavedSignatureDialog(documentId: Long, png: ByteArray, actions: SavedSignatureActions) {
    // A PNG of at most 1200 px: small enough to decode on the frame that opens the dialog, once.
    val bitmap = remember(png) { BitmapFactory.decodeByteArray(png, 0, png.size)?.asImageBitmap() }
    AlertDialog(
        onDismissRequest = actions.onCancel,
        title = { Text("Your signature") },
        text = {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(160.dp)
                    .background(Color.White, MaterialTheme.shapes.small)
                    .border(1.dp, MaterialTheme.colorScheme.outline, MaterialTheme.shapes.small)
                    .padding(12.dp),
                contentAlignment = Alignment.Center,
            ) {
                if (bitmap != null) {
                    Image(bitmap, contentDescription = "Your saved signature", contentScale = ContentScale.Fit)
                } else {
                    Text("This saved signature can no longer be shown.", color = Color.Black)
                }
            }
        },
        confirmButton = {
            Button(onClick = { actions.onUse(documentId) }, enabled = bitmap != null) { Text("Use") }
        },
        dismissButton = {
            Row {
                TextButton(onClick = actions.onDelete) { Text("Delete") }
                TextButton(onClick = actions.onDrawNew) { Text("Draw new") }
            }
        },
    )
}
