package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PictureWatermarkLook
import dev.vitela.pdf.core.WatermarkLayer
import dev.vitela.pdf.core.WatermarkLook
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The Add watermark dialog's rules, without a ViewModel: which pages, which text, which look. */
class WatermarkTest {
    private val document = WatermarkDocument(pageCount = 3)
    private val defaults = WatermarkDefaults(document.look, document.pictureLook)

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
        val look = WatermarkLook(fontSizePt = 60.0, opacity = 0.25, rotationDegrees = 45.0, layer = WatermarkLayer.UnderContent)
        val draft = WatermarkDefaults(look, document.pictureLook).draft()
        assertEquals(25.0, draft.opacityPercent, 1e-9)
        assertEquals(look, draft.look)
    }

    @Test
    fun aPicturesShareOfThePageIsAskedInPercent() {
        val picture = PictureWatermarkLook(scale = 0.4, opacity = 0.25, rotationDegrees = 10.0, layer = WatermarkLayer.UnderContent)
        val draft = WatermarkDefaults(document.look, picture).draft().switchedTo(WatermarkKind.Picture, WatermarkDefaults(document.look, picture))
        assertEquals(40.0, draft.scalePercent, 1e-9)
        // Opacity is shared by both kinds and starts at the text's, as on GTK and Windows.
        assertEquals(picture.copy(opacity = document.look.opacity), draft.pictureLook)
    }

    @Test
    fun theDialogOpensOnTextDrawnTheCoresTextWay() {
        val draft = defaults.draft()
        assertEquals(WatermarkKind.Text, draft.kind)
        assertNull(draft.picture)
        assertEquals(document.look, draft.look)
    }

    @Test
    fun switchingKindResetsTheAngleAndLayerToThatKindsDefaults() {
        val tilted = defaults.draft().copy(rotationDegrees = -20.0, layer = WatermarkLayer.UnderContent, text = "COPY", opacityPercent = 60.0)

        val picture = tilted.switchedTo(WatermarkKind.Picture, defaults)
        assertEquals(WatermarkKind.Picture, picture.kind)
        assertEquals(document.pictureLook.rotationDegrees, picture.rotationDegrees, 1e-9)
        assertEquals(document.pictureLook.layer, picture.layer)
        // Everything else the reader chose stays as they left it.
        assertEquals("COPY", picture.text)
        assertEquals(60.0, picture.opacityPercent, 1e-9)

        val text = picture.switchedTo(WatermarkKind.Text, defaults)
        assertEquals(document.look.rotationDegrees, text.rotationDegrees, 1e-9)
        assertEquals(document.look.layer, text.layer)
    }

    @Test
    fun switchingToTheKindAlreadyShowingChangesNothing() {
        val tilted = defaults.draft().copy(rotationDegrees = -20.0)
        assertEquals(tilted, tilted.switchedTo(WatermarkKind.Text, defaults))
    }

    @Test
    fun aPictureWithNothingChosenIsRefusedBeforeThePagesAreRead() {
        val draft = draft(pages = WatermarkPages.Custom, range = "9").switchedTo(WatermarkKind.Picture, defaults)
        assertEquals(refusal("Choose a PNG or JPEG picture to stamp on the pages."), planWatermark(document, draft, 0))
    }

    @Test
    fun aPictureIgnoresTheTextBox() {
        val draft = draft(text = "  ").switchedTo(WatermarkKind.Picture, defaults).copy(picture = WatermarkPicture(byteArrayOf(1)))
        assertEquals(PdfCoreResult.Success(listOf(0, 1, 2)), planWatermark(document, draft, 0))
    }

    @Test
    fun aChosenPictureSaysHowBigItIs() {
        assertEquals("Picture chosen (1 KB).", WatermarkPicture(ByteArray(10)).summary)
        assertEquals("Picture chosen (3 KB).", WatermarkPicture(ByteArray(3 * 1024 + 5)).summary)
    }

    @Test
    fun theCoresDefaultLooksSitInsideTheDialogsRanges() {
        val text = defaults.draft()
        assertTrue(text.fontSizePt in WATERMARK_FONT_SIZE_RANGE)
        assertTrue(text.opacityPercent in WATERMARK_OPACITY_PERCENT_RANGE)
        assertTrue(text.rotationDegrees in WATERMARK_ROTATION_RANGE)
        val picture = text.switchedTo(WatermarkKind.Picture, defaults)
        assertTrue(picture.scalePercent in WATERMARK_SCALE_PERCENT_RANGE)
        assertTrue(picture.rotationDegrees in WATERMARK_ROTATION_RANGE)
    }

    @Test
    fun theSummaryCountsPagesInWords() {
        assertEquals("Watermark added to 1 page.", watermarkSummary(1))
        assertEquals("Watermark added to 4 pages.", watermarkSummary(4))
    }

    private fun draft(text: String = "DRAFT", pages: WatermarkPages = WatermarkPages.All, range: String = "") =
        defaults.draft().copy(text = text, pages = pages, range = range)

    private fun refusal(message: String) = PdfCoreResult.Failure(PdfCoreError.Failed(message))
}
