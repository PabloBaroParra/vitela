package dev.vitela.pdf.recovery

import androidx.compose.foundation.layout.Row
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import java.time.ZoneId

/** Asks about one entry. Dismissing it is not a decision, so it keeps the work like **Later**. */
@Composable
fun RecoveryDialog(offer: RecoveryOffer, onRestore: () -> Unit, onDiscard: () -> Unit, onLater: () -> Unit) {
    AlertDialog(
        onDismissRequest = onLater,
        title = { Text("Recover unsaved work?") },
        text = {
            Text(
                "Vitela closed while \"${offer.name}\" had unsaved changes. A copy saved ${savedAtText(offer.savedAt, ZoneId.systemDefault())} " +
                    "can be restored as an unsaved document.\n\nUndo history is not recovered.",
            )
        },
        confirmButton = { Button(onClick = onRestore) { Text("Restore") } },
        dismissButton = {
            Row {
                TextButton(onClick = onDiscard) { Text("Discard") }
                TextButton(onClick = onLater) { Text("Later") }
            }
        },
    )
}
