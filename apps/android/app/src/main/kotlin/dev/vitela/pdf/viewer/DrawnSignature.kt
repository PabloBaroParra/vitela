package dev.vitela.pdf.viewer

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import java.io.ByteArrayOutputStream
import kotlin.math.ceil

internal const val SIGNATURE_STAMP_PROMPT = "Tap a page to place your signature."
internal const val SIGNATURE_PLACED = "Signature placed. Save to keep it."
internal const val SIGNATURE_UNRENDERABLE = "The signature could not be turned into an image."

/** Longest side of the PNG: the core places a stamp at most 144 pt long, so this is already ~600 DPI. */
internal const val SIGNATURE_MAX_SIDE_PX = 1200

/** A point on the signature pad, in pad pixels. */
internal data class PadPoint(val x: Float, val y: Float)

/** A stroke needs two points to leave a line; a bare tap draws nothing. */
internal fun hasSignatureInk(strokes: List<List<PadPoint>>): Boolean = strokes.any { it.size >= 2 }

/**
 * The part of the pad the PNG covers, in pad pixels: the ink's bounds grown
 * by half a stroke so round caps are not clipped. [scale] shrinks it so the
 * long side fits [SIGNATURE_MAX_SIDE_PX]; a small signature is never enlarged.
 */
internal data class SignatureFrame(val left: Float, val top: Float, val width: Float, val height: Float, val scale: Float) {
    val pixelWidth: Int get() = maxOf(1, ceil(width * scale).toInt())
    val pixelHeight: Int get() = maxOf(1, ceil(height * scale).toInt())
}

/** The crop around the ink, or null while the pad holds no line. */
internal fun signatureFrame(strokes: List<List<PadPoint>>, strokeWidth: Float, maxSidePx: Int = SIGNATURE_MAX_SIDE_PX): SignatureFrame? {
    val points = strokes.filter { it.size >= 2 }.flatten()
    if (points.isEmpty()) return null
    val half = strokeWidth / 2
    val left = points.minOf { it.x } - half
    val top = points.minOf { it.y } - half
    val width = points.maxOf { it.x } + half - left
    val height = points.maxOf { it.y } + half - top
    val scale = minOf(1f, maxSidePx / maxOf(width, height, 1f))
    return SignatureFrame(left, top, width, height, scale)
}

/**
 * The signature as a PNG with a transparent background — black ink, round
 * caps — cropped by [signatureFrame]. Null when there is no line to draw or
 * the bitmap cannot be encoded.
 */
internal fun signaturePng(strokes: List<List<PadPoint>>, strokeWidth: Float): ByteArray? {
    val frame = signatureFrame(strokes, strokeWidth) ?: return null
    val bitmap = Bitmap.createBitmap(frame.pixelWidth, frame.pixelHeight, Bitmap.Config.ARGB_8888)
    try {
        val canvas = Canvas(bitmap)
        canvas.scale(frame.scale, frame.scale)
        canvas.translate(-frame.left, -frame.top)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.BLACK
            style = Paint.Style.STROKE
            this.strokeWidth = strokeWidth
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
        }
        for (stroke in strokes) {
            if (stroke.size < 2) continue
            val path = Path().apply {
                moveTo(stroke[0].x, stroke[0].y)
                for (point in stroke.drop(1)) lineTo(point.x, point.y)
            }
            canvas.drawPath(path, paint)
        }
        return ByteArrayOutputStream().use { out ->
            if (bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)) out.toByteArray() else null
        }
    } finally {
        bitmap.recycle()
    }
}
