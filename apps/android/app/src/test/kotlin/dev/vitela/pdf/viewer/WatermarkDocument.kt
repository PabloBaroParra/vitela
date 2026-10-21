package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.PictureWatermarkLook
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit
import dev.vitela.pdf.core.WatermarkLayer
import dev.vitela.pdf.core.WatermarkLook

/**
 * A document that takes watermarks like the core: each accepted stamp is one
 * undo step, and the pages draw it only after [refreshPreview]. Its page
 * grammar is a toy — comma-separated one-based numbers — standing in for the
 * core's, whose own tests cover it.
 */
internal class WatermarkDocument(
    override val pageCount: Int = 3,
    private val editingAllowed: Boolean = true,
    private val refusal: String? = null,
) : PdfDocument {
    val look = WatermarkLook(fontSizePt = 72.0, opacity = 0.3, rotationDegrees = 45.0, layer = WatermarkLayer.OverContent)
    val pictureLook = PictureWatermarkLook(scale = 0.5, opacity = 0.25, rotationDegrees = 0.0, layer = WatermarkLayer.UnderContent)

    /** Each stamp the core accepted: the pages, the text and the look sent. */
    val stamps = mutableListOf<Triple<List<Int>, String, WatermarkLook>>()

    /** Each picture stamp the core accepted: the pages, the bytes and the look sent. */
    val pictureStamps = mutableListOf<Triple<List<Int>, ByteArray, PictureWatermarkLook>>()
    var previewRefreshes = 0
        private set

    override val pageSizes: List<PageSize> = List(pageCount) { PageSize(612.0, 792.0) }
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(0, 0, 0, ByteArray(0)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun annotations(): PdfCoreResult<AnnotationSnapshot> =
        PdfCoreResult.Success(AnnotationSnapshot(emptyList(), editingAllowed = true, canUndo = stamps.isNotEmpty() || pictureStamps.isNotEmpty(), canRedo = false))

    override fun refreshPreview(): PdfCoreResult<Unit> {
        previewRefreshes++
        return PdfCoreResult.Success(Unit)
    }

    override fun contentEditingAllowed(): Boolean = editingAllowed

    override fun defaultWatermarkLook(): PdfCoreResult<WatermarkLook> = PdfCoreResult.Success(look)

    override fun addWatermark(pages: List<Int>, text: String, look: WatermarkLook): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(PdfCoreError.Failed(it)) }
        stamps += Triple(pages, text, look)
        return PdfCoreResult.Success(Unit)
    }

    override fun defaultPictureWatermarkLook(): PdfCoreResult<PictureWatermarkLook> = PdfCoreResult.Success(pictureLook)

    override fun addImageWatermark(pages: List<Int>, image: ByteArray, look: PictureWatermarkLook): PdfCoreResult<Unit> {
        refusal?.let { return PdfCoreResult.Failure(PdfCoreError.Failed(it)) }
        pictureStamps += Triple(pages, image, look)
        return PdfCoreResult.Success(Unit)
    }

    override fun parsePageSelection(input: String): PdfCoreResult<List<Int>> {
        val pages = input.split(',').map { it.trim().toIntOrNull() ?: return PdfCoreResult.Failure(PdfCoreError.Failed("Type pages like 1-3,7.")) }
        pages.firstOrNull { it !in 1..pageCount }?.let { return PdfCoreResult.Failure(PdfCoreError.Failed("There is no page $it.")) }
        return PdfCoreResult.Success(pages.map { it - 1 }.distinct().sorted())
    }

    override fun close() = Unit
}
