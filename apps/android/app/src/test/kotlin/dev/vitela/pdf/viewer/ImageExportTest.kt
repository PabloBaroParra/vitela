package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.ImageExportFormat
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The plan: every choice checked before a folder is picked, every rule the core's. */
class ImageExportTest {
    @Test
    fun allPagesPlansEveryPageUnderTheCoresNames() {
        val plan = planned(PlanDocument(pageCount = 3), ImageExportDraft())
        assertEquals(listOf(0, 1, 2), plan.files.map { it.pageIndex })
        assertEquals(listOf("report-1.png", "report-2.png", "report-3.png"), plan.files.map { it.fileName })
        assertEquals(150, plan.dpi)
        assertEquals(ImageExportFormat.Png, plan.format)
    }

    @Test
    fun theCurrentPageIsPlannedAlone() {
        val plan = planned(PlanDocument(pageCount = 5), ImageExportDraft(pages = ImageExportPages.Current), currentPage = 3)
        assertEquals(listOf(3), plan.files.map { it.pageIndex })
    }

    @Test
    fun theCurrentPageIsClampedWhenTheViewerIsPastTheEnd() {
        // A page removal can leave the reader one past the last page until it catches up.
        val plan = planned(PlanDocument(pageCount = 2), ImageExportDraft(pages = ImageExportPages.Current), currentPage = 9)
        assertEquals(listOf(1), plan.files.map { it.pageIndex })
    }

    @Test
    fun aCustomRangeIsReadByTheCoreNotByTheShell() {
        val document = PlanDocument(pageCount = 9)
        val plan = planned(document, ImageExportDraft(pages = ImageExportPages.Custom, customRange = "2-3"))
        assertEquals(listOf(1, 2), plan.files.map { it.pageIndex })
        assertEquals(listOf("2-3"), document.parsed)
    }

    @Test
    fun aBadRangeIsRefusedWithTheCoresSentence() {
        val refusal = refused(PlanDocument(pageCount = 9), ImageExportDraft(pages = ImageExportPages.Custom, customRange = "nope"))
        assertEquals("\"nope\" is not a page number.", refusal)
    }

    @Test
    fun aResolutionOutsideTheOfferedRangeIsRefused() {
        val message = "Choose a resolution between 72 and 400 DPI."
        assertEquals(message, refused(PlanDocument(), ImageExportDraft(dpi = "71")))
        assertEquals(message, refused(PlanDocument(), ImageExportDraft(dpi = "401")))
    }

    @Test
    fun theBoundsOfTheResolutionRangeAreAccepted() {
        assertEquals(72, planned(PlanDocument(), ImageExportDraft(dpi = "72")).dpi)
        assertEquals(400, planned(PlanDocument(), ImageExportDraft(dpi = "400")).dpi)
    }

    @Test
    fun aClearedResolutionFallsBackToTheDefault() {
        assertEquals(150, planned(PlanDocument(), ImageExportDraft(dpi = "")).dpi)
    }

    @Test
    fun aPageTooLargeForTheResolutionIsNamedUpFront() {
        val refusal = refused(PlanDocument(pageCount = 4, oversizedPage = 2), ImageExportDraft(dpi = "400"))
        assertEquals("Page 3 is too large to export at 400 DPI. Choose a lower resolution.", refusal)
    }

    @Test
    fun aDocumentThatWithholdsExtractionIsRefused() {
        val refusal = refused(PlanDocument(allowed = false), ImageExportDraft())
        assertEquals(IMAGE_EXPORT_NOT_ALLOWED, refusal)
    }

    @Test
    fun aDocumentWithoutPagesHasNothingToExport() {
        assertEquals("This document has no pages to export.", refused(PlanDocument(pageCount = 0), ImageExportDraft()))
    }

    @Test
    fun theFormatTravelsIntoTheNames() {
        val plan = planned(PlanDocument(pageCount = 1), ImageExportDraft(format = ImageExportFormat.Jpeg))
        assertEquals(listOf("report-1.jpg"), plan.files.map { it.fileName })
        assertEquals(ImageExportFormat.Jpeg, plan.format)
    }

    private fun planned(document: PdfDocument, draft: ImageExportDraft, currentPage: Int = 0): ImageExportPlan {
        val result = planImageExport(document, "report.pdf", currentPage, draft)
        assertTrue("expected a plan but got $result", result is PdfCoreResult.Success)
        return (result as PdfCoreResult.Success).value
    }

    private fun refused(document: PdfDocument, draft: ImageExportDraft): String {
        val result = planImageExport(document, "report.pdf", 0, draft)
        assertTrue("expected a refusal but got $result", result is PdfCoreResult.Failure)
        return ((result as PdfCoreResult.Failure).error as PdfCoreError.Failed).message
    }
}

/** A document that answers the export questions the way the core does, so the plan and the export loop can be tested without PDFium. */
internal class PlanDocument(
    override val pageCount: Int = 3,
    private val allowed: Boolean = true,
    private val oversizedPage: Int? = null,
    private val failingPage: Int? = null,
) : PdfDocument {
    val parsed = mutableListOf<String>()
    val exported = mutableListOf<Triple<Int, Int, ImageExportFormat>>()
    override val pageSizes = List(pageCount) { PageSize(612.0, 792.0) }
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun imageExportAllowed(): Boolean = allowed
    override fun parsePageSelection(input: String): PdfCoreResult<List<Int>> {
        parsed += input
        val range = Regex("""(\d+)-(\d+)""").matchEntire(input)
            ?: return PdfCoreResult.Failure(PdfCoreError.Failed("\"$input\" is not a page number."))
        return PdfCoreResult.Success((range.groupValues[1].toInt()..range.groupValues[2].toInt()).map { it - 1 })
    }
    override fun pageImageFileName(displayName: String, pageIndex: Int, format: ImageExportFormat): PdfCoreResult<String> =
        PdfCoreResult.Success("${displayName.removeSuffix(".pdf")}-${pageIndex + 1}.${if (format == ImageExportFormat.Png) "png" else "jpg"}")
    override fun firstPageTooLargeToExport(pages: List<Int>, dpi: Int): PdfCoreResult<Int?> =
        PdfCoreResult.Success(pages.firstOrNull { it == oversizedPage && dpi > 300 })
    override fun exportPageImage(pageIndex: Int, dpi: Int, format: ImageExportFormat): PdfCoreResult<ByteArray> {
        if (pageIndex == failingPage) return PdfCoreResult.Failure(PdfCoreError.Failed("The document could not be processed."))
        exported += Triple(pageIndex, dpi, format)
        return PdfCoreResult.Success(byteArrayOf(pageIndex.toByte()))
    }
    override fun close() = Unit
}
