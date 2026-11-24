package dev.vitela.pdf.recovery

import androidx.compose.foundation.layout.Row
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import dev.vitela.pdf.R
import java.time.ZoneId

/** Asks about one entry. Dismissing it is not a decision, so it keeps the work like **Later**. */
@Composable
fun RecoveryDialog(offer: RecoveryOffer, onRestore: () -> Unit, onDiscard: () -> Unit, onLater: () -> Unit) {
    AlertDialog(
        onDismissRequest = onLater,
        title = { Text(stringResource(R.string.recovery_title)) },
        text = {
            Text(stringResource(R.string.recovery_detail, offer.name, savedAtText(offer.savedAt, ZoneId.systemDefault())))
        },
        confirmButton = { Button(onClick = onRestore) { Text(stringResource(R.string.recovery_restore)) } },
        dismissButton = {
            Row {
                TextButton(onClick = onDiscard) { Text(stringResource(R.string.recovery_discard)) }
                TextButton(onClick = onLater) { Text(stringResource(R.string.recovery_later)) }
            }
        },
    )
}
