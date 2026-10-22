import XCTest
@testable import Vitela

/// Viewport tiles follow Linux's `layout.rs` `tile_plan`: only past the page
/// bitmap's pixel budget, on a fixed 1024-pixel grid anchored to the page,
/// covering the part in view and nothing already drawn or on its way.
final class ViewerStoreTileTests: XCTestCase {
    private let letter = PageDimensions(width: 612, height: 792)

    private func store() -> (FakePdfCoreClient, ViewerStore) {
        let client = FakePdfCoreClient(pages: [letter, letter])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return (client, store)
    }

    /// The top-left 400 × 300 points of the page.
    private let corner = PageViewRect(x: 0, y: 0, width: 400, height: 300)

    func testAPageInsideItsPixelBudgetIsNotTiled() {
        let (_, store) = store()
        store.setRenderScale(2)
        store.setZoom(2) // 288 DPI: Letter is 7.75 MP, inside 8 MP
        XCTAssertNil(store.tilePlan(page: 0, zoom: 2, visible: corner))
        XCTAssertNil(store.beginTiles(page: 0, visible: corner))
        XCTAssertNil(store.pageTiles[0])
    }

    func testPastTheBudgetTheViewIsCoveredByGridTilesAtTheFullDpi() throws {
        let (client, store) = store()
        store.setRenderScale(2)
        store.setZoom(4) // 576 DPI asked, the bitmap held to 292
        let request = try XCTUnwrap(store.beginTiles(page: 0, visible: corner))
        XCTAssertEqual(request.dpi, 576)
        // 400 × 300 pt at 8 px/pt is 3200 × 2400 px: four columns, three rows.
        XCTAssertEqual(request.tiles.count, 12)
        XCTAssertEqual(request.tiles.first, PixelRect(left: 0, top: 0, width: 1024, height: 1024))
        XCTAssertEqual(request.tiles.last, PixelRect(left: 3072, top: 2048, width: 1024, height: 1024))

        XCTAssertNil(store.beginTiles(page: 0, visible: corner), "tiles on their way are not asked for twice")
        store.applyTiles(store.tilesResult(for: request), for: request)
        XCTAssertEqual(store.pageTiles[0]?.images.count, 12)
        XCTAssertNil(store.beginTiles(page: 0, visible: corner), "a scroll inside drawn tiles renders nothing")
        XCTAssertEqual(client.tileRequests.count, 1)
    }

    func testTheLastRowAndColumnStopAtThePageEdge() throws {
        let (_, store) = store()
        store.setRenderScale(2)
        store.setZoom(4)
        let bottomRight = PageViewRect(x: 520, y: 700, width: 92, height: 92)
        let request = try XCTUnwrap(store.beginTiles(page: 0, visible: bottomRight))
        // 612 × 792 pt at 8 px/pt is 4896 × 6336 px.
        XCTAssertEqual(request.tiles, [PixelRect(left: 4096, top: 5120, width: 800, height: 1024),
                                       PixelRect(left: 4096, top: 6144, width: 800, height: 192)])
    }

    func testScrollingAsksOnlyForTheNewTilesAndDropsTheOnesThatLeft() throws {
        let (client, store) = store()
        store.setRenderScale(2)
        store.setZoom(4)
        let first = try XCTUnwrap(store.beginTiles(page: 0, visible: PageViewRect(x: 0, y: 0, width: 100, height: 100)))
        store.applyTiles(store.tilesResult(for: first), for: first)
        let moved = try XCTUnwrap(store.beginTiles(page: 0, visible: PageViewRect(x: 0, y: 140, width: 100, height: 100)))
        XCTAssertEqual(moved.tiles, [PixelRect(left: 0, top: 1024, width: 1024, height: 1024)])
        XCTAssertEqual(store.pageTiles[0]?.images.keys.map { $0 }, [], "the tile that left the view is not kept")
        XCTAssertEqual(client.tileRequests.count, 1, "nothing is rendered until the request runs")
    }

    func testTilesOfAnOldZoomOrLayoutNeverLand() throws {
        let (_, store) = store()
        store.setRenderScale(2)
        store.setZoom(4)
        let stale = try XCTUnwrap(store.beginTiles(page: 0, visible: corner))
        store.setZoom(3.5) // 504 DPI: a new layer
        _ = store.beginTiles(page: 0, visible: corner)
        store.applyTiles(store.tilesResult(for: stale), for: stale)
        XCTAssertEqual(store.pageTiles[0]?.dpi, 504)
        XCTAssertEqual(store.pageTiles[0]?.images.count, 0)

        let current = try XCTUnwrap(store.pageTiles[0].map { _ in store.tilePlan(page: 0, zoom: 3.5, visible: corner) } ?? nil)
        XCTAssertEqual(current.dpi, 504)
        store.showPages([letter, letter])
        XCTAssertTrue(store.pageTiles.isEmpty, "a re-laid-out document starts without tiles")
    }

    func testAFailedBatchLeavesThePageBitmapAndIsAskedForAgain() throws {
        let (client, store) = store()
        store.setRenderScale(2)
        store.setZoom(4)
        client.tileError = ViewerFailure.renderFailed(page: 0, message: "boom")
        let failed = try XCTUnwrap(store.beginTiles(page: 0, visible: corner))
        store.applyTiles(store.tilesResult(for: failed), for: failed)
        XCTAssertEqual(store.pageTiles[0]?.images.count, 0)
        XCTAssertNotNil(store.beginTiles(page: 0, visible: corner), "the next look asks again")
    }

    func testZoomingBackInsideTheBudgetForgetsTheTiles() throws {
        let (_, store) = store()
        store.setRenderScale(2)
        store.setZoom(4)
        let request = try XCTUnwrap(store.beginTiles(page: 0, visible: corner))
        store.applyTiles(store.tilesResult(for: request), for: request)
        store.setZoom(1)
        XCTAssertNil(store.beginTiles(page: 0, visible: corner))
        XCTAssertNil(store.pageTiles[0])
    }
}
