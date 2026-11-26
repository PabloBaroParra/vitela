package dev.vitela.pdf.document

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import java.io.File

/** How handing a shared copy to the system went. */
enum class ShareOutcome { Shown, NoReceiver, Failed }

/**
 * Hands a copy written by [ShareCopies] to the system share sheet. The file
 * goes out through the app's `FileProvider` with a read-only grant; the
 * receiver never learns a path, and nothing outside `cache/share/` is
 * reachable through that authority.
 */
object ShareSheet {
    private const val PDF = "application/pdf"

    /** The `FileProvider` authority declared in the manifest. */
    fun authority(context: Context): String = "${context.packageName}.share"

    fun show(context: Context, copy: File, title: String): ShareOutcome {
        val uri = try {
            FileProvider.getUriForFile(context, authority(context), copy)
        } catch (error: IllegalArgumentException) {
            return ShareOutcome.Failed
        }
        val send = Intent(Intent.ACTION_SEND)
            .setType(PDF)
            .putExtra(Intent.EXTRA_STREAM, uri)
            .putExtra(Intent.EXTRA_TITLE, title)
            // The ClipData carries the grant into the chooser and on to the target.
            .also { it.clipData = ClipData.newUri(context.contentResolver, title, uri) }
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        return try {
            context.startActivity(Intent.createChooser(send, null))
            ShareOutcome.Shown
        } catch (error: ActivityNotFoundException) {
            ShareOutcome.NoReceiver
        } catch (error: RuntimeException) {
            // A SecurityException or the like: the status says so, the app stays up.
            ShareOutcome.Failed
        }
    }
}
