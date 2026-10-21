import XCTest
@testable import Vitela

/// Add watermark against the real core: the stamp reaches the preview as
/// one undo step, and a character Helvetica cannot show is refused in the
/// reader's words before anything is recorded.
final class UniFfiWatermarkIntegrationTests: XCTestCase {
    func testAStampOnTheSampleIsFoundOnThePageAndUndoneInOneStep() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let document = try XCTUnwrap(store.document)
        let look = store.client.defaultWatermarkLook()
        XCTAssertTrue(try store.client.search(document: document, query: "QZXWATERMARK").isEmpty)

        let (problem, request) = store.beginWatermark(
            pages: .current, text: "QZXWATERMARK", fontSize: look.fontSize,
            opacityPercent: look.opacityPercent, rotationDegrees: look.rotationDegrees
        )
        XCTAssertNil(problem)
        let work = try XCTUnwrap(request)
        store.applyPageWork(store.pageWorkResult(for: work), for: work)

        XCTAssertEqual(store.editStatus, "Watermark added to 1 page. Changes are pending save.")
        XCTAssertEqual(try store.client.search(document: document, query: "QZXWATERMARK").map(\.pageIndex), [0])
        XCTAssertTrue(store.client.undo(document: document))
        XCTAssertFalse(store.client.canUndo(document: document))
    } }

    func testTextHelveticaCannotShowIsRefusedAndNothingIsRecorded() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())

        let (problem, request) = store.beginWatermark(
            pages: .all, text: "下書き", fontSize: 60, opacityPercent: 30, rotationDegrees: 45
        )
        XCTAssertNil(problem)
        let work = try XCTUnwrap(request)
        store.applyPageWork(store.pageWorkResult(for: work), for: work)

        XCTAssertTrue(store.editStatus.hasPrefix("The watermark font cannot show “"), store.editStatus)
        XCTAssertFalse(store.canUndo)
    } }

    func testTheStartingLookIsTheCoresDiagonal() {
        let look = UniFfiPdfCoreClient().defaultWatermarkLook()
        XCTAssertEqual(look.fontSize, 60)
        XCTAssertEqual(look.opacityPercent, 30, accuracy: 0.0001)
        XCTAssertEqual(look.rotationDegrees, 45)
    }
}
