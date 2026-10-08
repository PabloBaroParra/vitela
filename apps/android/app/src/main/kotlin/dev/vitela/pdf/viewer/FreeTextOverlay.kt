package dev.vitela.pdf.viewer

import android.graphics.Color
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Typeface
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas

private val freeTextPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
    color = Color.BLACK
    // Helvetica's closest neighbour on the phone. Widths differ a little, so a line's end may
    // sit a few points off the saved file's; where lines break never does — the core chose them.
    typeface = Typeface.SANS_SERIF
}

/**
 * Paints [drawing] the way the saved page shows it: each of the core's lines at
 * its own baseline in the box's frame, which [FreeTextDrawing.frame] maps onto
 * the page with its turn and zoom, clipped to the box. Text is drawn in the
 * frame's points, so the matrix is what scales it.
 */
internal fun DrawScope.drawFreeText(drawing: FreeTextDrawing) {
    val frame = drawing.frame
    val matrix = Matrix().apply {
        setValues(
            floatArrayOf(
                frame.a.toFloat(), frame.c.toFloat(), frame.e.toFloat(),
                frame.b.toFloat(), frame.d.toFloat(), frame.f.toFloat(),
                0f, 0f, 1f,
            ),
        )
    }
    freeTextPaint.textSize = drawing.fontSizePt.toFloat()
    drawIntoCanvas { canvas ->
        val native = canvas.nativeCanvas
        native.save()
        native.concat(matrix)
        native.clipRect(0f, 0f, drawing.width.toFloat(), drawing.height.toFloat())
        drawing.lines.forEach { line -> native.drawText(line.text, line.xPt.toFloat(), line.baselineFromTopPt.toFloat(), freeTextPaint) }
        native.restore()
    }
}
