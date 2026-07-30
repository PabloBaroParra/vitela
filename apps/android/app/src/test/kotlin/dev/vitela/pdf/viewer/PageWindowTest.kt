package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PageSize
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The continuous reader keeps only a window of pages decoded. These are the
 * rules that keep a long document from decoding itself into an OOM: at
 * 144 dpi a Letter page is ~7.7 MB as ARGB_8888, so "how many pages may be
 * resident" is a memory budget, not a preference.
 */
class PageWindowTest {
    @Test
    fun pageWindow_extendsTheVisibleRangeByTheRadius() {
        assertEquals(2..7, pageWindow(first = 4, last = 5, pageCount = 20, radius = 2))
    }

    @Test
    fun pageWindow_staysWithinTheDocument() {
        assertEquals(0..2, pageWindow(first = 0, last = 0, pageCount = 3, radius = 5))
    }

    @Test
    fun pageWindow_isEmptyWithoutPages() {
        assertEquals(IntRange.EMPTY, pageWindow(first = 0, last = 0, pageCount = 0, radius = 1))
    }

    @Test
    fun retainWindow_dropsEverythingOutsideTheWindow() {
        val cached = mapOf(0 to "a", 3 to "b", 4 to "c", 9 to "d")
        assertEquals(mapOf(3 to "b", 4 to "c"), retainWindow(cached, 3..5))
    }

    @Test
    fun pageBitmaps_keepBridgesBoundedAndRemoveThemWhenReplaced() {
        val invalidated = invalidatePageBitmaps(
            PageBitmaps(pages = mapOf(1 to "current", 4 to "outside"), bridges = mapOf(0 to "older")),
            0..2,
        )
        assertEquals(emptyMap<Int, String>(), invalidated.pages)
        assertEquals(mapOf(0 to "older", 1 to "current"), invalidated.bridges)

        val replaced = replacePageBitmap(invalidated, pageIndex = 1, page = "sharp")
        assertEquals(mapOf(1 to "sharp"), replaced.pages)
        assertEquals(mapOf(0 to "older"), replaced.bridges)
        assertEquals(PageBitmaps(mapOf(1 to "sharp"), emptyMap()), retainPageBitmaps(replaced, 1..1))
    }

    @Test
    fun renderOrder_prioritizesVisiblePagesAndRejectsStaleCompletions() {
        assertEquals(listOf(4, 5, 3, 6), renderOrder(first = 4, last = 5, pageCount = 20))
        assertTrue(acceptsRenderCompletion(startedGeneration = 3, currentGeneration = 3, pageIndex = 4, window = 2..6))
        assertTrue(!acceptsRenderCompletion(startedGeneration = 3, currentGeneration = 4, pageIndex = 4, window = 2..6))
    }

    @Test
    fun cacheWindowIsWiderThanTheRenderWindow() {
        // Prefetch decides what gets rasterized ahead of the scroll; the cache
        // window decides what survives once it scrolls off. A cache narrower
        // than the prefetch would evict pages the very tick they finish.
        assertTrue("cache=$CACHE_PAGES must exceed prefetch=$PREFETCH_PAGES", CACHE_PAGES > PREFETCH_PAGES)
    }

    @Test
    fun aspectRatio_usesThePageBox() {
        assertEquals(612f / 792f, PageSize(612.0, 792.0).aspectRatio(), 0.0001f)
    }

    @Test
    fun aspectRatio_fallsBackOnADegeneratePageBox() {
        // A zero or negative MediaBox would make the placeholder's
        // Modifier.aspectRatio throw; the default keeps the list laying out.
        assertEquals(DEFAULT_PAGE_ASPECT_RATIO, PageSize(0.0, 792.0).aspectRatio(), 0.0001f)
        assertEquals(DEFAULT_PAGE_ASPECT_RATIO, PageSize(612.0, -1.0).aspectRatio(), 0.0001f)
    }

    @Test
    fun renderDpi_fitsThePageToTheAvailableWidth() {
        // A 612 pt wide page across 1224 px is exactly 2x, i.e. 144 dpi.
        assertEquals(144, renderDpi(PageSize(612.0, 792.0), availableWidthPx = 1224))
        // Half as wide a viewport asks for half the density, not a fixed one.
        assertEquals(72, renderDpi(PageSize(612.0, 792.0), availableWidthPx = 612))
    }

    @Test
    fun renderDpi_scalesWithCustomZoomBeforeApplyingTheCaps() {
        val size = PageSize(612.0, 792.0)
        // 144 dpi fit-to-width (see renderDpi_fitsThePageToTheAvailableWidth)
        // scaled by 1.25x zoom. 180 dpi stays under this page's ~206.8 dpi
        // pixel-budget ceiling, so the cap is not what produces this number.
        assertEquals(180, renderDpi(size, availableWidthPx = 1224, zoomFactor = 1.25))

        val dpi = renderDpi(size, availableWidthPx = 1224, zoomFactor = MAX_ZOOM_FACTOR)
        val pixels = (612.0 * dpi / 72.0).toLong() * (792.0 * dpi / 72.0).toLong()
        assertTrue("$dpi dpi rasterizes $pixels px, over the budget", pixels <= MAX_PAGE_PIXELS)
    }

    @Test
    fun renderDpi_staysInsideThePixelBudgetOnAWideViewport() {
        // Fitting a Letter page to 10000 px would want ~1176 dpi and a 129 Mpx
        // raster — half a gigabyte for one page. The budget has to bite here,
        // and still spend most of what it is allowed.
        val dpi = renderDpi(PageSize(612.0, 792.0), availableWidthPx = 10_000)
        val pixels = (612.0 * dpi / 72.0).toLong() * (792.0 * dpi / 72.0).toLong()
        assertTrue("$dpi dpi rasterizes $pixels px, over the budget", pixels <= MAX_PAGE_PIXELS)
        assertTrue("$dpi dpi wastes the budget at $pixels px", pixels > MAX_PAGE_PIXELS * 0.9)
    }

    @Test
    fun renderDpi_fallsBackBeforeTheReaderHasBeenMeasured() {
        assertEquals(FALLBACK_RENDER_DPI, renderDpi(PageSize(612.0, 792.0), availableWidthPx = 0))
    }

    @Test
    fun renderDpi_fallsBackOnAnUnusablePageBox() {
        assertEquals(FALLBACK_RENDER_DPI, renderDpi(null, availableWidthPx = 1080))
        assertEquals(FALLBACK_RENDER_DPI, renderDpi(PageSize(0.0, 792.0), availableWidthPx = 1080))
    }

    @Test
    fun renderDpi_clampsADegenerateMediaBoxToTheCeiling() {
        // A 1 pt wide page fit to 1080 px asks for 77760 dpi. Unclamped that is
        // an unbounded raster request handed straight to PDFium.
        assertEquals(MAX_RENDER_DPI, renderDpi(PageSize(1.0, 1.0), availableWidthPx = 1080))
    }

    @Test
    fun dominantPage_isTheOneCoveringMostOfTheViewport() {
        // Viewport 0..1000. Page 4 shows its last 200 px, page 5 the other 800.
        val visible = listOf(VisiblePage(4, -800, 1000), VisiblePage(5, 200, 1000))
        assertEquals(5, dominantPage(visible, viewportStart = 0, viewportEnd = 1000))
    }

    @Test
    fun dominantPage_reachesTheLastPageAtTheEndOfTheDocument() {
        // The regression this exists for: scrolled to the bottom of a 3-page
        // document, page 2 keeps a sliver on screen, so "first visible" would
        // report page 2 forever and the counter could never say 3 of 3.
        val visible = listOf(VisiblePage(1, -900, 1000), VisiblePage(2, 100, 1000))
        assertEquals(2, dominantPage(visible, viewportStart = 0, viewportEnd = 1000))
    }

    @Test
    fun dominantPage_favoursTheEarlierPageOnATie() {
        // Half and half: the counter advances only once the next page has
        // actually taken over, not the instant it draws even.
        val visible = listOf(VisiblePage(1, -500, 1000), VisiblePage(2, 500, 1000))
        assertEquals(1, dominantPage(visible, viewportStart = 0, viewportEnd = 1000))
    }

    @Test
    fun dominantPage_isNullWithNothingOnScreen() {
        assertEquals(null, dominantPage(emptyList(), viewportStart = 0, viewportEnd = 1000))
    }

    @Test
    fun visibleRangeStatus_readsAsARangeOnlyWhenMoreThanOnePageShows() {
        assertEquals("Page 1 of 10.", visibleRangeStatus(0, 0, 10))
        assertEquals("Pages 2-4 of 10.", visibleRangeStatus(1, 3, 10))
        assertEquals("No pages.", visibleRangeStatus(0, 0, 0))
    }
}
