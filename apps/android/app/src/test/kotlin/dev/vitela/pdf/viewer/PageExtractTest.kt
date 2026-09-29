package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The plan: both gates and the range are the core's answers, asked before a destination is picked. */
class PageExtractTest {
    @Test
    fun aTypedRangePlansTheCoresPages() {
        val document = ExtractDocument(pageCount = 9)
        val plan = planned(document, "2-4")
        assertEquals(listOf(1, 2, 3), plan.pages)
        assertEquals(false, plan.sourceIsSigned)
        assertEquals(listOf("2-4"), document.parsed)
    }

    @Test
    fun aSignedSourceIsCarriedIntoThePlan() {
        assertTrue(planned(ExtractDocument(signed = true), "1-1").sourceIsSigned)
    }

    @Test
    fun theExtractionBitIsAskedBeforeTheRange() {
        val document = ExtractDocument(extractionAllowed = false)
        assertEquals(PAGE_EXTRACT_NOT_ALLOWED, refused(document, "1-2"))
        assertEquals(emptyList<String>(), document.parsed)
    }

    @Test
    fun aDocumentThatCannotBeRewrittenIsRefusedBeforeTheRange() {
        val document = ExtractDocument(rewriteAllowed = false)
        assertEquals(PAGE_EXTRACT_NO_REWRITE, refused(document, "1-2"))
        assertEquals(emptyList<String>(), document.parsed)
    }

    @Test
    fun aBlankRangeAsksForOneInsteadOfReachingTheCore() {
        val document = ExtractDocument()
        assertEquals("Type which pages to extract, for example 1-3,7.", refused(document, "   "))
        assertEquals(emptyList<String>(), document.parsed)
    }

    @Test
    fun aBadRangeIsRefusedInTheCoresOwnWords() {
        assertEquals("\"nope\" is not a page number.", refused(ExtractDocument(), "nope"))
    }

    @Test
    fun anEmptyDocumentHasNothingToExtract() {
        assertEquals("This document has no pages to extract.", refused(ExtractDocument(pageCount = 0), "1"))
    }

    @Test
    fun theSuggestedNameKeepsTheStemAndMarksTheExtract() {
        assertEquals("report-extract.pdf", extractFileName("report.pdf"))
        assertEquals("Scan-extract.pdf", extractFileName("Scan.PDF"))
        assertEquals("notes-extract.pdf", extractFileName("notes"))
        assertEquals("Document-extract.pdf", extractFileName("  "))
    }

    @Test
    fun theSummaryCountsPagesAndWarnsAboutASignedSource() {
        assertEquals("Extracted 1 page to a new PDF.", pageExtractSummary(1, sourceIsSigned = false))
        assertEquals("Extracted 3 pages to a new PDF.", pageExtractSummary(3, sourceIsSigned = false))
        assertEquals(
            "Extracted 2 pages to a new PDF. The original document is signed, so the extracted PDF's signature no longer verifies.",
            pageExtractSummary(2, sourceIsSigned = true),
        )
    }

    private fun planned(document: PdfDocument, range: String): PageExtractPlan =
        (planPageExtract(document, range) as PdfCoreResult.Success).value

    private fun refused(document: PdfDocument, range: String): String =
        ((planPageExtract(document, range) as PdfCoreResult.Failure).error as PdfCoreError.Failed).message
}

internal class ExtractDocument(
    override val pageCount: Int = 3,
    private val extractionAllowed: Boolean = true,
    private val rewriteAllowed: Boolean = true,
    private val signed: Boolean = false,
    private val failure: String? = null,
) : PdfDocument {
    val parsed = mutableListOf<String>()
    val extracted = mutableListOf<List<Int>>()
    override val pageSizes = List(pageCount) { PageSize(612.0, 792.0) }
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun pageExtractionAllowed(): Boolean = extractionAllowed
    override fun fullRewriteAllowed(): Boolean = rewriteAllowed
    override fun extractSourceIsSigned(): Boolean = signed
    override fun parsePageSelection(input: String): PdfCoreResult<List<Int>> {
        parsed += input
        val range = Regex("""(\d+)-(\d+)""").matchEntire(input)
            ?: return PdfCoreResult.Failure(PdfCoreError.Failed("\"$input\" is not a page number."))
        return PdfCoreResult.Success((range.groupValues[1].toInt()..range.groupValues[2].toInt()).map { it - 1 })
    }
    override fun extractPages(pages: List<Int>): PdfCoreResult<ByteArray> {
        failure?.let { return PdfCoreResult.Failure(PdfCoreError.Failed(it)) }
        extracted += pages
        return PdfCoreResult.Success(pages.map { it.toByte() }.toByteArray())
    }
    override fun close() = Unit
}
