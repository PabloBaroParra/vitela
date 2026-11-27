package dev.vitela.pdf.viewer

import android.graphics.Bitmap
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.RenderedPage
import java.io.ByteArrayOutputStream

internal fun RenderedPage.toImageBitmap(): ImageBitmap? = toBitmap()?.asImageBitmap()

/** A render with its pixels already converted; null in the success when they cannot be. Call off the main thread. */
internal fun PdfCoreResult<RenderedPage>.toPageBitmap(): PdfCoreResult<ImageBitmap?> = when (this) {
    is PdfCoreResult.Success -> PdfCoreResult.Success(value.toImageBitmap())
    is PdfCoreResult.Failure -> this
}

/** The page as a PNG, for a preview kept on disk; null when it cannot be encoded. */
internal fun RenderedPage.toPng(): ByteArray? {
    val bitmap = toBitmap() ?: return null
    val out = ByteArrayOutputStream()
    return try {
        if (bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)) out.toByteArray() else null
    } finally {
        bitmap.recycle()
    }
}

private fun RenderedPage.toBitmap(): Bitmap? {
    if (width <= 0 || height <= 0 || stride < width * 4 || rgba.size < stride * height) return null
    val pixels = IntArray(width * height)
    for (y in 0 until height) {
        val row = y * stride
        for (x in 0 until width) {
            val offset = row + x * 4
            pixels[y * width + x] = ((rgba[offset].toInt() and 0xff) shl 16) or
                ((rgba[offset + 1].toInt() and 0xff) shl 8) or
                (rgba[offset + 2].toInt() and 0xff) or
                ((rgba[offset + 3].toInt() and 0xff) shl 24)
        }
    }
    return Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888)
}
