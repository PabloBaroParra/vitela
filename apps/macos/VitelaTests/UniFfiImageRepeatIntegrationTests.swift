import XCTest
@testable import Vitela

/// Images edited again before a save, against the real core (B25 T-207:
/// the core folds a second geometry edit into the queued one).
final class UniFfiImageRepeatIntegrationTests: XCTestCase {
    /// Against the real core an image is moved, then resized and deleted
    /// before any save; the move and the resize fold into one undo step.
    func testAnImageIsEditedAgainBeforeASave() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: UniFfiContentEditIntegrationTests.onePhotoPdf())
        store.setContentEditMode(true)
        func read() throws -> [ContentImage] {
            let request = try XCTUnwrap(store.beginPageContent(page: 0))
            store.applyPageContent(store.pageContentResult(for: request), for: request)
            return try XCTUnwrap(store.pageContent[0]).images
        }
        func drag(from: PdfPoint, to: PdfPoint) throws {
            XCTAssertTrue(store.beginContentPress(page: 0, at: from, zoom: 1))
            guard case let .move(request) = store.endContentDrag(at: to) else { return XCTFail("the drag records an edit") }
            store.applyPageWork(store.pageWorkResult(for: request), for: request)
            _ = try read()
        }
        _ = try read()
        try drag(from: PdfPoint(x: 100, y: 620), to: PdfPoint(x: 200, y: 520))
        let moved = try XCTUnwrap(store.selectedContentImage)
        XCTAssertEqual(moved.bbox.x, 172, accuracy: 0.5)
        try drag(from: PdfPoint(x: moved.bbox.x + moved.bbox.width, y: moved.bbox.y + moved.bbox.height),
                 to: PdfPoint(x: moved.bbox.x + moved.bbox.width + 50, y: moved.bbox.y + moved.bbox.height + 25))
        XCTAssertEqual(try XCTUnwrap(store.selectedContentImage).bbox.width, 150, accuracy: 0.5)

        store.undo()
        if let refresh = store.beginPageRefresh() { store.applyPageWork(store.pageWorkResult(for: refresh), for: refresh) }
        XCTAssertEqual(try XCTUnwrap(try read().first).bbox.x, 72, accuracy: 0.5, "one undo step for the move and the resize")

        try drag(from: PdfPoint(x: 100, y: 620), to: PdfPoint(x: 110, y: 620))
        let request = try XCTUnwrap(store.beginSelectedImageDelete())
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
        XCTAssertEqual(try read(), [], "deleted right after a move, before a save")
    } }
}
