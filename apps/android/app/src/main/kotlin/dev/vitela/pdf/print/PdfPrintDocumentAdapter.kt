package dev.vitela.pdf.print

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import android.os.Bundle
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.print.PageRange
import android.print.PrintAttributes
import android.print.PrintDocumentAdapter
import android.print.PrintDocumentInfo
import android.print.pdf.PrintedPdfDocument
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.cancel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.nio.ByteBuffer
import java.util.concurrent.Executors
import kotlin.coroutines.cancellation.CancellationException

/**
 * Fixed rasterization DPI for printing, like GTK and Windows: each page is
 * rendered once at print quality and scaled onto the sheet, whatever the
 * paper. Does not depend on the viewer's zoom.
 */
private const val PRINT_DPI = 300

/**
 * Prints [document] — a throwaway copy of the saved snapshot, see
 * `ViewerViewModel.printDocument` — by rasterizing the pages the job asks
 * for at [PRINT_DPI] onto a [PrintedPdfDocument]. This adapter owns
 * [document] from construction: it is closed in [onFinish].
 *
 * The framework calls this on the main thread, so every render runs on one
 * private worker. A single thread keeps the core's document single-threaded
 * and lets the close in [onFinish] queue behind a render still in flight.
 */
class PdfPrintDocumentAdapter(
    private val context: Context,
    private val document: PdfDocument,
    private val name: String,
) : PrintDocumentAdapter() {
    private val worker = Executors.newSingleThreadExecutor()
    private val scope = CoroutineScope(SupervisorJob() + worker.asCoroutineDispatcher())
    private var attributes: PrintAttributes? = null

    @Volatile
    private var finished = false

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
        attributes = newAttributes
        callback.onLayoutFinished(
            PrintDocumentInfo.Builder(name)
                .setContentType(PrintDocumentInfo.CONTENT_TYPE_DOCUMENT)
                .setPageCount(document.pageCount)
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
        val layout = attributes
        if (layout == null || finished) {
            destination.close()
            callback.onWriteFailed("The print layout is not ready.")
            return
        }
        val requested = pagesToPrint(pages.map { it.start..it.end }, document.pageCount)
        val job = scope.launch {
            try {
                writeSheets(layout, requested, destination, cancellationSignal)
            } catch (cancelled: CancellationException) {
                // Cancelled by the system's signal; a cancel from onFinish needs no answer.
                if (!finished) withContext(NonCancellable + Dispatchers.Main) { callback.onWriteCancelled() }
                throw cancelled
            } catch (error: Exception) {
                withContext(Dispatchers.Main) { callback.onWriteFailed(error.message ?: "Could not render the PDF for printing.") }
                return@launch
            }
            withContext(Dispatchers.Main) {
                callback.onWriteFinished(writtenRanges(requested).map { PageRange(it.first, it.last) }.toTypedArray())
            }
        }
        cancellationSignal?.setOnCancelListener { job.cancel() }
    }

    override fun onFinish() {
        finished = true
        scope.cancel()
        // Queued behind any render still running, so the core never sees a close mid-render.
        worker.execute { document.close() }
        worker.shutdown()
    }

    /** One sheet per requested page, in order, then the finished PDF to [destination]. */
    private suspend fun writeSheets(
        layout: PrintAttributes,
        pages: List<Int>,
        destination: ParcelFileDescriptor,
        cancellationSignal: CancellationSignal?,
    ) {
        val sheets = PrintedPdfDocument(context, layout)
        try {
            for ((sheetIndex, pageIndex) in pages.withIndex()) {
                currentCoroutineContext().ensureActive()
                if (cancellationSignal?.isCanceled == true) throw CancellationException("Print cancelled")
                val sheet = sheets.startPage(sheetIndex)
                drawPage(sheet.canvas, sheet.info.contentRect, pageIndex)
                sheets.finishPage(sheet)
            }
            ParcelFileDescriptor.AutoCloseOutputStream(destination).use { sheets.writeTo(it) }
        } finally {
            sheets.close()
            destination.close()
        }
    }

    /**
     * Rasterizes [pageIndex] at [PRINT_DPI] and scales it into [content],
     * centred. A page that cannot be rendered (over the core's raster
     * ceiling, say) or turned into a bitmap is left blank rather than
     * failing the whole job, as on GTK.
     */
    private fun drawPage(canvas: Canvas, content: Rect, pageIndex: Int) {
        val rendered = (document.renderPage(pageIndex, PRINT_DPI) as? PdfCoreResult.Success)?.value ?: return
        val bitmap = try {
            rendered.toBitmap()
        } catch (_: OutOfMemoryError) {
            null
        } ?: return
        try {
            val placement = fitCentered(bitmap.width.toFloat(), bitmap.height.toFloat(), content.width().toFloat(), content.height().toFloat()) ?: return
            canvas.save()
            canvas.translate(content.left + placement.dx, content.top + placement.dy)
            canvas.scale(placement.scale, placement.scale)
            canvas.drawBitmap(bitmap, 0f, 0f, Paint(Paint.FILTER_BITMAP_FLAG))
            canvas.restore()
        } finally {
            bitmap.recycle()
        }
    }
}

/**
 * The core's RGBA rows as an ARGB_8888 bitmap (same byte order in memory),
 * or null when the buffer does not match its own dimensions. Copied straight
 * from the buffer: at 300 DPI a page is ~35 MB, too much to also unpack
 * through an `IntArray`.
 */
private fun RenderedPage.toBitmap(): Bitmap? {
    if (width <= 0 || height <= 0 || stride < width * 4 || rgba.size < stride * height) return null
    val packed = if (stride == width * 4) rgba else ByteArray(width * height * 4).also { out ->
        for (y in 0 until height) System.arraycopy(rgba, y * stride, out, y * width * 4, width * 4)
    }
    return Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888).also { it.copyPixelsFromBuffer(ByteBuffer.wrap(packed)) }
}
