import XCTest
@testable import Vitela

/// The reader's page DPI follows Linux's `layout.rs` `resolve_page_box`: the
/// zoom times the screen's backing scale, inside 24–600 DPI and 8 MP.
final class ViewerStoreRenderDpiTests: XCTestCase {
    private func store(_ pages: [PageDimensions] = [PageDimensions(width: 612, height: 792)]) -> (FakePdfCoreClient, ViewerStore) {
        let client = FakePdfCoreClient(pages: pages)
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return (client, store)
    }

    private func render(_ store: ViewerStore, page: Int = 0) throws {
        let request = try XCTUnwrap(store.beginRender(page: page))
        store.applyRender(store.renderResult(for: request), for: request)
    }

    func testARetinaScreenGetsOneBitmapPixelPerScreenPixel() throws {
        let (client, store) = store()
        store.setRenderScale(2)
        try render(store)
        XCTAssertEqual(client.requestedDpi, [144], "100% on a 2× screen is 144 DPI, not a 72 DPI bitmap stretched")

        store.setRenderScale(2)
        XCTAssertNil(store.beginRender(page: 0), "the same scale leaves the page current")

        store.setRenderScale(1)
        try render(store)
        XCTAssertEqual(client.requestedDpi, [144, 72], "moving to a 1× screen re-renders at its own DPI")
        XCTAssertEqual(store.pageSlots[0].renderDpi, 72)
    }

    func testAScaleThatIsNoScaleCountsAsOne() {
        let (_, store) = store()
        for scale in [0, -2, .nan, .infinity] {
            store.setRenderScale(scale)
            XCTAssertEqual(store.renderScale, 1)
        }
    }

    func testABitmapPastEightMegapixelsIsRenderedCoarserNotLarger() {
        let (_, store) = store()
        store.setRenderScale(2)
        // Letter at 400% on 2×: 576 DPI would be 31 MP; Linux's
        // `MAX_BASE_PIXELS` brings it down to 292 DPI, 2482 × 3212.
        let dpi = store.dpi(forPage: 0, zoom: 4)
        XCTAssertEqual(dpi, 292)
        XCTAssertLessThanOrEqual(612.0 * Double(dpi) / 72 * (792.0 * Double(dpi) / 72), ViewerStore.maxPagePixels)
    }

    func testTheDpiStaysInsideLinuxsBounds() {
        let (_, store) = store([PageDimensions(width: 10, height: 10)])
        store.setRenderScale(3)
        XCTAssertEqual(store.dpi(forPage: 0, zoom: 4), 600, "MAX_RENDER_DPI")
        store.setRenderScale(0.5)
        XCTAssertEqual(store.dpi(forPage: 0, zoom: 0.5), 24, "MIN_RENDER_DPI")
    }
}
