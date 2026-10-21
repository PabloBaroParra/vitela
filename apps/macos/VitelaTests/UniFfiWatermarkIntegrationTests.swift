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
        let look = store.client.defaultWatermarkLook(.text)
        XCTAssertTrue(try store.client.search(document: document, query: "QZXWATERMARK").isEmpty)

        let (problem, request) = store.beginWatermark(
            pages: .current, mark: .text("QZXWATERMARK", fontSize: look.size),
            opacityPercent: look.opacityPercent, rotationDegrees: look.rotationDegrees, layer: look.layer
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
            pages: .all, mark: .text("下書き", fontSize: 60), opacityPercent: 30, rotationDegrees: 45, layer: .overContent
        )
        XCTAssertNil(problem)
        let work = try XCTUnwrap(request)
        store.applyPageWork(store.pageWorkResult(for: work), for: work)

        XCTAssertTrue(store.editStatus.hasPrefix("The watermark font cannot show “"), store.editStatus)
        XCTAssertFalse(store.canUndo)
    } }

    func testEachKindStartsFromTheCoresLook() {
        let client = UniFfiPdfCoreClient()
        let text = client.defaultWatermarkLook(.text), picture = client.defaultWatermarkLook(.picture)
        XCTAssertEqual(text.size, 60)
        XCTAssertEqual(text.opacityPercent, 30, accuracy: 0.0001)
        XCTAssertEqual(text.rotationDegrees, 45)
        XCTAssertEqual(text.layer, .overContent)
        XCTAssertEqual(picture.size, 50, accuracy: 0.0001)
        XCTAssertEqual(picture.rotationDegrees, 0)
        XCTAssertEqual(picture.layer, .underContent)
    }

    func testAPictureStampOnTheSampleIsOneUndoStepAndBytesThatAreNotOneAreRefused() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())

        let (_, refused) = store.beginWatermark(
            pages: .all, mark: .picture(Data("not a picture".utf8), scalePercent: 50), opacityPercent: 30, rotationDegrees: 0, layer: .underContent
        )
        let bad = try XCTUnwrap(refused)
        store.applyPageWork(store.pageWorkResult(for: bad), for: bad)
        XCTAssertEqual(store.editStatus, Watermarking.notAPicture)
        XCTAssertFalse(store.canUndo)

        let (problem, request) = store.beginWatermark(
            pages: .current, mark: .picture(try UniFfiContentEditIntegrationTests.bluePng(), scalePercent: 50), opacityPercent: 30, rotationDegrees: 0, layer: .underContent
        )
        XCTAssertNil(problem)
        let work = try XCTUnwrap(request)
        store.applyPageWork(store.pageWorkResult(for: work), for: work)
        XCTAssertEqual(store.editStatus, "Watermark added to 1 page. Changes are pending save.")
        XCTAssertTrue(store.canUndo)
    } }
}
