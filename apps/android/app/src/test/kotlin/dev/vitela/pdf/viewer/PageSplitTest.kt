package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit
import dev.vitela.pdf.core.SplitPart
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The plan: both gates are asked first, then the cuts and the part names are the core's answer. */
class PageSplitTest {
    @Test
    fun typedCutsPlanTheCoresPartsNamedAfterTheTitle() {
        val document = SplitDocument(pageCount = 10)
        val plan = planned(document, "3,7")
        assertEquals(
            listOf(SplitPart(0, 2, "report.pdf-part1.pdf"), SplitPart(3, 6, "report.pdf-part2.pdf"), SplitPart(7, 9, "report.pdf-part3.pdf")),
            plan.parts,
        )
        assertEquals(false, plan.sourceIsSigned)
        assertEquals(listOf("3,7" to "report.pdf"), document.planned)
    }

    @Test
    fun aSignedSourceIsCarriedIntoThePlan() {
        assertTrue(planned(SplitDocument(signed = true), "1").sourceIsSigned)
    }

    @Test
    fun theExtractionBitIsAskedBeforeTheCuts() {
        val document = SplitDocument(extractionAllowed = false)
        assertEquals(PAGE_EXTRACT_NOT_ALLOWED, refused(document, "1"))
        assertEquals(emptyList<Pair<String, String>>(), document.planned)
    }

    @Test
    fun aDocumentThatCannotBeRewrittenIsRefusedBeforeTheCuts() {
        val document = SplitDocument(rewriteAllowed = false)
        assertEquals(PAGE_SPLIT_NO_REWRITE, refused(document, "1"))
        assertEquals(emptyList<Pair<String, String>>(), document.planned)
    }

    @Test
    fun badCutsAreRefusedInTheCoresOwnWords() {
        assertEquals("Type where to cut, for example 3 to split after page 3.", refused(SplitDocument(), " "))
    }

    @Test
    fun theSummaryCountsPartsAndWarnsAboutASignedSource() {
        assertEquals("Split into 3 PDFs.", pageSplitSummary(3, sourceIsSigned = false))
        assertEquals(
            "Split into 2 PDFs. The original document is signed, so the signature in each part no longer verifies.",
            pageSplitSummary(2, sourceIsSigned = true),
        )
    }

    private fun planned(document: PdfDocument, cuts: String): PageSplitPlan =
        (planPageSplit(document, "report.pdf", cuts) as PdfCoreResult.Success).value

    private fun refused(document: PdfDocument, cuts: String): String =
        ((planPageSplit(document, "report.pdf", cuts) as PdfCoreResult.Failure).error as PdfCoreError.Failed).message
}

/**
 * A document whose split plan follows the core's shape: cut after each typed
 * page, parts named `<title>-partN.pdf`. [failAtPart] makes the extraction of
 * that part (one-based) fail, as a prune refusal would.
 */
internal class SplitDocument(
    override val pageCount: Int = 4,
    private val extractionAllowed: Boolean = true,
    private val rewriteAllowed: Boolean = true,
    private val signed: Boolean = false,
    private val failAtPart: Int? = null,
) : PdfDocument {
    val planned = mutableListOf<Pair<String, String>>()
    val extracted = mutableListOf<List<Int>>()
    override val pageSizes = List(pageCount) { PageSize(612.0, 792.0) }
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun pageExtractionAllowed(): Boolean = extractionAllowed
    override fun fullRewriteAllowed(): Boolean = rewriteAllowed
    override fun extractSourceIsSigned(): Boolean = signed
    override fun planSplit(cuts: String, displayName: String): PdfCoreResult<List<SplitPart>> {
        planned += cuts to displayName
        if (cuts.isBlank()) return PdfCoreResult.Failure(PdfCoreError.Failed("Type where to cut, for example 3 to split after page 3."))
        val after = cuts.split(",").map { it.trim().toIntOrNull() ?: return PdfCoreResult.Failure(PdfCoreError.Failed("\"$it\" is not a page number.")) }
        val bounds = listOf(0) + after + pageCount
        return PdfCoreResult.Success(bounds.zipWithNext().mapIndexed { index, (from, to) -> SplitPart(from, to - 1, "$displayName-part${index + 1}.pdf") })
    }
    override fun extractPages(pages: List<Int>): PdfCoreResult<ByteArray> {
        if (failAtPart == extracted.size + 1) return PdfCoreResult.Failure(PdfCoreError.Failed("The document could not be processed."))
        extracted += pages
        return PdfCoreResult.Success(pages.map { it.toByte() }.toByteArray())
    }
    override fun close() = Unit
}
