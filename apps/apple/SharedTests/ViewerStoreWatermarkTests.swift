import XCTest
@testable import Vitela

/// Add watermark's rules: which pages, what is refused before the core is
/// asked, and the one undoable step it records.
final class ViewerStoreWatermarkTests: XCTestCase {
    private func opened(pages: Int = 3) -> (FakePdfCoreClient, ViewerStore) {
        let client = FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: pages))
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return (client, store)
    }

    private func stamp(
        _ store: ViewerStore, pages: ViewerStore.WatermarkPages = .all, text: String = "DRAFT",
        size: Double = 60, opacity: Double = 30, angle: Double = 45
    ) -> String? {
        let (problem, request) = store.beginWatermark(
            pages: pages, text: text, fontSize: size, opacityPercent: opacity, rotationDegrees: angle
        )
        if let request { store.applyPageWork(store.pageWorkResult(for: request), for: request) }
        return problem
    }

    func testAllPagesStampsEveryPageAsOneUndoableStep() {
        let (client, store) = opened()

        XCTAssertNil(stamp(store, size: 72, opacity: 50, angle: -30))

        XCTAssertEqual(client.contentEdits, [
            .addWatermark(Watermark(pages: [0, 1, 2], text: "DRAFT", fontSize: 72, opacity: 0.5, rotationDegrees: -30)),
        ])
        XCTAssertEqual(store.editStatus, "Watermark added to 3 pages. Changes are pending save.")
        XCTAssertTrue(store.canUndo)
    }

    func testTheCurrentPageIsClampedToTheDocument() {
        let (client, store) = opened()
        store.currentPage = 7

        XCTAssertNil(stamp(store, pages: .current))

        guard case let .addWatermark(watermark) = client.contentEdits.first else { return XCTFail("nothing stamped") }
        XCTAssertEqual(watermark.pages, [2])
        XCTAssertEqual(store.editStatus, "Watermark added to 1 page. Changes are pending save.")
    }

    func testATypedRangeIsReadByTheSharedGrammarAndABadOneKeepsTheSheetOpen() {
        let (client, store) = opened()

        XCTAssertEqual(stamp(store, pages: .range("  ")), Watermarking.blankRange)
        XCTAssertEqual(stamp(store, pages: .range("9")), "\"9\" is not a page range in this document.")
        XCTAssertTrue(client.contentEdits.isEmpty)

        XCTAssertNil(stamp(store, pages: .range("1,3")))
        guard case let .addWatermark(watermark) = client.contentEdits.first else { return XCTFail("nothing stamped") }
        XCTAssertEqual(watermark.pages, [0, 2])
    }

    func testBlankTextIsRefusedAndTypedTextIsKeptAsTyped() {
        let (client, store) = opened()

        XCTAssertEqual(stamp(store, text: "   "), Watermarking.blankText)
        XCTAssertTrue(client.contentEdits.isEmpty)

        XCTAssertNil(stamp(store, text: " COPY"))
        guard case let .addWatermark(watermark) = client.contentEdits.first else { return XCTFail("nothing stamped") }
        XCTAssertEqual(watermark.text, " COPY")
    }

    func testALookOutsideTheDialogsRangesIsRefused() {
        let (client, store) = opened()

        XCTAssertEqual(stamp(store, size: 4), "Size must be between 8 and 200 pt.")
        XCTAssertEqual(stamp(store, opacity: 0), "Opacity must be between 5 and 100 %.")
        XCTAssertEqual(stamp(store, angle: 270), "Angle must be between -180 and 180°.")
        XCTAssertEqual(stamp(store, size: .nan), "Size must be between 8 and 200 pt.")
        XCTAssertTrue(client.contentEdits.isEmpty)
    }

    func testADocumentThatForbidsContentChangesIsRefused() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.editingAllowed = false
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        XCTAssertEqual(stamp(store), ContentEditing.forbidden)
        XCTAssertTrue(client.contentEdits.isEmpty)
    }

    func testTheCoresRefusalComesBackAsTheWorksFailure() {
        let (client, store) = opened()
        client.contentEditError = EditFailure.failed(Watermarking.encodingGap("€"))

        XCTAssertNil(stamp(store, text: "€"))

        XCTAssertEqual(store.editStatus, "The watermark font cannot show “€”. Use Latin letters, digits and punctuation.")
        XCTAssertFalse(store.canUndo)
    }

    func testTheDialogsStartingLookSitsInsideItsRanges() {
        let look = FakePdfCoreClient(pages: []).defaultWatermarkLook()
        XCTAssertTrue(Watermarking.fontSizes.contains(look.fontSize))
        XCTAssertTrue(Watermarking.opacityPercents.contains(look.opacityPercent))
        XCTAssertTrue(Watermarking.rotations.contains(look.rotationDegrees))
    }
}
