package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PageSize
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Low-resolution previews stand in for a page that has nothing sharp to show
 * yet. They are a memory budget like the sharp cache: a fixed width, a pixel
 * ceiling, and a window around the current page.
 */
class PagePreviewsTest {
    @Test
    fun previewDpi_fitsALetterPageToThePreviewWidth() {
        val dpi = previewDpi(PageSize(612.0, 792.0))

        assertEquals(23, dpi)
        assertTrue(612.0 / 72.0 * dpi <= PREVIEW_WIDTH_PX)
    }

    @Test
    fun previewDpi_capsATallPageByThePixelCeiling() {
        val size = PageSize(100.0, 2000.0)
        val dpi = previewDpi(size)
        val pixels = (size.widthPt / 72.0 * dpi) * (size.heightPt / 72.0 * dpi)

        assertTrue(pixels <= PREVIEW_MAX_PIXELS)
        assertTrue(dpi < PREVIEW_WIDTH_PX / size.widthPt * 72.0)
    }

    @Test
    fun previewDpi_fallsBackToLetterOnAnUnusablePageBox() {
        val letter = previewDpi(PageSize(612.0, 792.0))

        assertEquals(letter, previewDpi(null))
        assertEquals(letter, previewDpi(PageSize(0.0, 792.0)))
    }

    @Test
    fun previewWindow_isBoundedAroundTheCurrentPage() {
        assertEquals(60 - PREVIEW_PAGES..60 + PREVIEW_PAGES, previewWindow(current = 60, pageCount = 500))
        assertEquals(0..PREVIEW_PAGES, previewWindow(current = 0, pageCount = 500))
        assertEquals(IntRange.EMPTY, previewWindow(current = 0, pageCount = 0))
    }

    @Test
    fun nextPreview_takesTheVisiblePagesFirst() {
        assertEquals(4, nextPreview(visible = 4..6, current = 5, window = 0..20) { false })
        assertEquals(6, nextPreview(visible = 4..6, current = 5, window = 0..20) { it < 6 })
    }

    @Test
    fun nextPreview_thenTheNearestToTheCurrentPage_aheadBeforeBehind() {
        val done = setOf(4, 5, 6)

        assertEquals(7, nextPreview(visible = 4..6, current = 5, window = 0..20) { it in done })
        assertEquals(3, nextPreview(visible = 4..6, current = 5, window = 0..20) { it in done + 7 })
        assertEquals(9, nextPreview(visible = 4..6, current = 5, window = 0..20) { it in done + setOf(7, 3, 8, 2) })
    }

    @Test
    fun nextPreview_neverLeavesTheWindow() {
        assertEquals(2, nextPreview(visible = 0..3, current = 1, window = 2..3) { false })
        assertEquals(3, nextPreview(visible = 0..0, current = 0, window = 2..3) { it == 2 })
        assertNull(nextPreview(visible = 0..1, current = 1, window = 0..3) { true })
        assertNull(nextPreview(visible = 0..1, current = 0, window = IntRange.EMPTY) { false })
    }

    @Test
    fun acceptsPreviewCompletion_onlyForItsGenerationAndWindow() {
        assertTrue(acceptsPreviewCompletion(startedGeneration = 3, currentGeneration = 3, pageIndex = 5, window = 0..10))
        assertTrue(!acceptsPreviewCompletion(startedGeneration = 2, currentGeneration = 3, pageIndex = 5, window = 0..10))
        assertTrue(!acceptsPreviewCompletion(startedGeneration = 3, currentGeneration = 3, pageIndex = 11, window = 0..10))
    }
}
