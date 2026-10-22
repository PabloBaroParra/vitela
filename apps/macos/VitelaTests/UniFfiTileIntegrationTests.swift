import XCTest
@testable import Vitela

/// Viewport tiles against the real core: `render_page_tiles` returns one
/// bitmap per tile asked for, each exactly the tile's size, with the page's
/// ink in it.
final class UniFfiTileIntegrationTests: XCTestCase {
    func testTilesComeBackTheSizeOfTheirGridCellsWithThePageDrawnInThem() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        store.setRenderScale(2)
        store.setZoom(4)
        let size = try XCTUnwrap(store.pageSlots.first?.dimensions)
        let whole = PageViewRect(x: 0, y: 0, width: size.width, height: size.height)

        let request = try XCTUnwrap(store.beginTiles(page: 0, visible: whole))
        let images = try store.tilesResult(for: request).get()

        XCTAssertEqual(images.count, request.tiles.count)
        for (tile, image) in zip(request.tiles, images) {
            XCTAssertEqual(image.width, tile.width)
            XCTAssertEqual(image.height, tile.height)
        }
        let inked = images.contains { image in
            image.rgba.withUnsafeBytes { bytes in
                stride(from: 0, to: bytes.count, by: 4).contains { bytes[$0] < 200 }
            }
        }
        XCTAssertTrue(inked, "the tiles must carry the page's content, not blank paper")

        store.applyTiles(.success(images), for: request)
        XCTAssertEqual(store.pageTiles[0]?.images.count, request.tiles.count)
    } }
}
