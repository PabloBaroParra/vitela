package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.WatermarkLook
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The Add watermark dialog's rules, without a ViewModel: which pages, which text, which look. */
class WatermarkTest {
    private val document = WatermarkDocument(pageCount = 3)

    @Test
    fun allPagesNamesEveryPageInOrder() {
        assertEquals(PdfCoreResult.Success(listOf(0, 1, 2)), planWatermark(document, draft(), currentPage = 1))
    }

    @Test
    fun theCurrentPageIsClampedToTheDocument() {
        assertEquals(PdfCoreResult.Success(listOf(1)), planWatermark(document, draft(pages = WatermarkPages.Current), currentPage = 1))
        // The readout can sit one past the end right after a deletion.
        assertEquals(PdfCoreResult.Success(listOf(2)), planWatermark(document, draft(pages = WatermarkPages.Current), currentPage = 7))
    }

    @Test
    fun aCustomRangeIsReadByTheCoresGrammar() {
        assertEquals(PdfCoreResult.Success(listOf(0, 2)), planWatermark(document, draft(pages = WatermarkPages.Custom, range = "1,3"), 0))
    }

    @Test
    fun aBadCustomRangeSurfacesTheCoresOwnSentence() {
        assertEquals(refusal("There is no page 9."), planWatermark(document, draft(pages = WatermarkPages.Custom, range = "9"), 0))
    }

    @Test
    fun aBlankCustomRangeAsksForOneInWords() {
        assertEquals(refusal("Type which pages to stamp, for example 1-3,7."), planWatermark(document, draft(pages = WatermarkPages.Custom, range = "  "), 0))
    }

    @Test
    fun blankTextIsRefusedBeforeThePagesAreRead() {
        assertEquals(refusal("Type the text to stamp on the pages."), planWatermark(document, draft(text = "   ", pages = WatermarkPages.Custom, range = "9"), 0))
    }

    @Test
    fun aDocumentWithNoPagesIsRefusedWhateverTheChoice() {
        for (pages in WatermarkPages.entries) {
            assertEquals(refusal("This document has no pages to stamp."), planWatermark(WatermarkDocument(pageCount = 0), draft(pages = pages, range = "1"), 0))
        }
    }

    @Test
    fun opacityIsAskedInPercentAndRoundTripsTheCoresLook() {
        val look = WatermarkLook(fontSizePt = 60.0, opacity = 0.25, rotationDegrees = 45.0)
        val draft = WatermarkDraft.from(look)
        assertEquals(25.0, draft.opacityPercent, 1e-9)
        assertEquals(look, draft.look)
    }

    @Test
    fun theCoresDefaultLookSitsInsideTheDialogsRanges() {
        val draft = WatermarkDraft.from(WatermarkDocument().look)
        assertTrue(draft.fontSizePt in WATERMARK_FONT_SIZE_RANGE)
        assertTrue(draft.opacityPercent in WATERMARK_OPACITY_PERCENT_RANGE)
        assertTrue(draft.rotationDegrees in WATERMARK_ROTATION_RANGE)
    }

    @Test
    fun theSummaryCountsPagesInWords() {
        assertEquals("Watermark added to 1 page.", watermarkSummary(1))
        assertEquals("Watermark added to 4 pages.", watermarkSummary(4))
    }

    private fun draft(text: String = "DRAFT", pages: WatermarkPages = WatermarkPages.All, range: String = "") =
        WatermarkDraft.from(document.look).copy(text = text, pages = pages, range = range)

    private fun refusal(message: String) = PdfCoreResult.Failure(PdfCoreError.Failed(message))
}
