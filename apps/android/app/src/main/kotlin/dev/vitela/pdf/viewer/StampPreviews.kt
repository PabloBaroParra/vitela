package dev.vitela.pdf.viewer

import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.graphics.Paint
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The id the core gave the stamp an insert just added: the one stamp in
 * [after] that [before] lacked. Null when there is not exactly one — the
 * stamp then keeps its outline rather than borrow another's picture. The
 * Windows shell reconciles the same way (`StampPreviewReconciliation`).
 */
internal fun insertedStampId(before: List<Annotation>, after: List<Annotation>): Long? {
    val previous = before.mapTo(HashSet()) { it.id }
    return after.filter { it.kind == AnnotationKind.Stamp && it.id !in previous }.singleOrNull()?.id
}

/**
 * [images] decoded once each, off the main thread. Hoisted above the page
 * slots, so scrolling a page back in does not decode its stamps again; an
 * entry not decoded yet — or not decodable — is simply absent, and its stamp
 * draws as an outline meanwhile.
 */
@Composable
internal fun rememberStampBitmaps(images: Map<Long, ByteArray>): Map<Long, ImageBitmap> {
    val decoded by produceState(emptyMap<Long, ImageBitmap>(), images) {
        val kept = value.filterKeys { it in images }
        val missing = images.filterKeys { it !in kept }
        value = kept + withContext(Dispatchers.Default) {
            missing.mapNotNull { (id, bytes) -> BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.let { id to it.asImageBitmap() } }
        }
    }
    return decoded
}

private val stampPaint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)

/**
 * Paints [image] into a stamp's [rect] the way the saved page shows it. The
 * picture's top edge is the rect's PDF top, and a `/Rotate` turns it along
 * with the page, so the three corners are placed as page-space points and the
 * bitmap is mapped onto them — an axis-aligned blit would stand the stamp
 * upright on a turned page where the file lays it on its side.
 */
internal fun DrawScope.drawStampImage(image: ImageBitmap, rect: AnnotationRect, placement: PagePlacement) {
    val top = rect.y + rect.height
    val topLeft = placement.placePoint(AnnotationPoint(rect.x, top))
    val topRight = placement.placePoint(AnnotationPoint(rect.x + rect.width, top))
    val bottomLeft = placement.placePoint(AnnotationPoint(rect.x, rect.y))
    val width = image.width.toFloat()
    val height = image.height.toFloat()
    val matrix = Matrix()
    val mapped = matrix.setPolyToPoly(
        floatArrayOf(0f, 0f, width, 0f, 0f, height), 0,
        floatArrayOf(
            topLeft.x.toFloat(), topLeft.y.toFloat(),
            topRight.x.toFloat(), topRight.y.toFloat(),
            bottomLeft.x.toFloat(), bottomLeft.y.toFloat(),
        ), 0,
        3,
    )
    if (!mapped) return
    drawIntoCanvas { it.nativeCanvas.drawBitmap(image.asAndroidBitmap(), matrix, stampPaint) }
}
