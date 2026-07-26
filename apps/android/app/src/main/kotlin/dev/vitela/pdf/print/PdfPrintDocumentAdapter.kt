package dev.vitela.pdf.print

import android.os.Bundle
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.print.PageRange
import android.print.PrintAttributes
import android.print.PrintDocumentAdapter
import android.print.PrintDocumentInfo

/** Sends the original selected PDF bytes to Android's print service. */
class PdfPrintDocumentAdapter(private val bytes: ByteArray, private val name: String) : PrintDocumentAdapter() {
    override fun onLayout(
        oldAttributes: PrintAttributes?,
        newAttributes: PrintAttributes,
        cancellationSignal: CancellationSignal?,
        callback: LayoutResultCallback,
        extras: Bundle?,
    ) {
        if (cancellationSignal?.isCanceled == true) {
            callback.onLayoutCancelled()
            return
        }
        callback.onLayoutFinished(
            PrintDocumentInfo.Builder(name)
                .setContentType(PrintDocumentInfo.CONTENT_TYPE_DOCUMENT)
                .setPageCount(PrintDocumentInfo.PAGE_COUNT_UNKNOWN)
                .build(),
            oldAttributes != newAttributes,
        )
    }

    override fun onWrite(
        pages: Array<out PageRange>,
        destination: ParcelFileDescriptor,
        cancellationSignal: CancellationSignal?,
        callback: WriteResultCallback,
    ) {
        try {
            if (cancellationSignal?.isCanceled == true) {
                callback.onWriteCancelled()
                return
            }
            ParcelFileDescriptor.AutoCloseOutputStream(destination).use { it.write(bytes) }
            callback.onWriteFinished(arrayOf(PageRange.ALL_PAGES))
        } catch (error: Exception) {
            callback.onWriteFailed(error.message ?: "Could not write PDF for printing.")
        }
    }
}
