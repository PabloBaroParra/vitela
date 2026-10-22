import XCTest
@testable import Vitela

/// Page bitmaps further than three pages from the ones laid out are let go,
/// as Linux's `CACHE_PAGES` and Windows' `PageWindow` do.
final class ViewerStorePageCacheTests: XCTestCase {
    private func store(pages: Int = 12) throws -> ViewerStore {
        let store = ViewerStore(client: FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: pages)))
        store.open(bytes: Data([1]))
        for page in 0..<pages {
            let request = try XCTUnwrap(store.beginRender(page: page))
            store.applyRender(store.renderResult(for: request), for: request)
        }
        return store
    }

    private func spans(_ pages: ClosedRange<Int>) -> [Int: PageViewportSpan] {
        Dictionary(uniqueKeysWithValues: pages.map { ($0, PageViewportSpan(top: Double($0) * 800, bottom: Double($0) * 800 + 792)) })
    }

    func testPagesMoreThanThreeAwayFromTheOnesShownGiveUpTheirBitmaps() throws {
        let store = try store()
        store.releasePages(farFrom: spans(5...6))

        let kept = store.pageSlots.indices.filter { store.pageSlots[$0].image != nil }
        XCTAssertEqual(kept, Array(2...9))
        XCTAssertEqual(store.pageSlots[0].status, .idle)
        XCTAssertNil(store.pageSlots[11].renderZoom)
    }

    func testAReleasedPageRendersAgainWhenItComesBack() throws {
        let store = try store()
        store.releasePages(farFrom: spans(10...11))
        XCTAssertNotNil(store.beginRender(page: 0), "a released page is stale, not current")
        XCTAssertNil(store.beginRender(page: 10), "a kept page stays current")
    }

    func testNothingIsReleasedWithoutAPageInView() throws {
        let store = try store()
        store.releasePages(farFrom: [:])
        store.releasePages(farFrom: [40: PageViewportSpan(top: 0, bottom: 1)])
        XCTAssertTrue(store.pageSlots.allSatisfy { $0.image != nil })
    }
}
