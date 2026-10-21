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
        size: Double = 60, opacity: Double = 30, angle: Double = 45, layer: WatermarkLayer = .overContent
    ) -> String? {
        stamp(store, pages: pages, mark: .text(text, fontSize: size), opacity: opacity, angle: angle, layer: layer)
    }

    private func stamp(
        _ store: ViewerStore, pages: ViewerStore.WatermarkPages = .all, mark: ViewerStore.WatermarkMarkAnswer,
        opacity: Double = 30, angle: Double = 0, layer: WatermarkLayer = .underContent
    ) -> String? {
        let (problem, request) = store.beginWatermark(
            pages: pages, mark: mark, opacityPercent: opacity, rotationDegrees: angle, layer: layer
        )
        if let request { store.applyPageWork(store.pageWorkResult(for: request), for: request) }
        return problem
    }

    func testAllPagesStampsEveryPageAsOneUndoableStep() {
        let (client, store) = opened()

        XCTAssertNil(stamp(store, size: 72, opacity: 50, angle: -30))

        XCTAssertEqual(client.contentEdits, [
            .addWatermark(Watermark(pages: [0, 1, 2], mark: .text("DRAFT", fontSize: 72), opacity: 0.5, rotationDegrees: -30, layer: .overContent)),
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
        XCTAssertEqual(watermark.mark, .text(" COPY", fontSize: 60))
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
        let client = FakePdfCoreClient(pages: [])
        let text = client.defaultWatermarkLook(.text), picture = client.defaultWatermarkLook(.picture)
        XCTAssertTrue(Watermarking.fontSizes.contains(text.size))
        XCTAssertTrue(Watermarking.scalePercents.contains(picture.size))
        for look in [text, picture] {
            XCTAssertTrue(Watermarking.opacityPercents.contains(look.opacityPercent))
            XCTAssertTrue(Watermarking.rotations.contains(look.rotationDegrees))
        }
    }

    func testAPictureIsStampedWithItsBytesUntouchedItsSizeAsAFractionAndItsLayer() {
        let (client, store) = opened()

        XCTAssertNil(stamp(store, pages: .range("2"), mark: .picture(Data([7, 8, 9]), scalePercent: 40), layer: .underContent))

        XCTAssertEqual(client.contentEdits, [
            .addWatermark(Watermark(pages: [1], mark: .picture(Data([7, 8, 9]), scale: 0.4), opacity: 0.3, rotationDegrees: 0, layer: .underContent)),
        ])
        XCTAssertEqual(store.editStatus, "Watermark added to 1 page. Changes are pending save.")
    }

    func testNoPictureChosenOrASizeOutsideTheRangeIsRefused() {
        let (client, store) = opened()

        XCTAssertEqual(stamp(store, mark: .picture(nil, scalePercent: 50)), Watermarking.noPicture)
        XCTAssertEqual(stamp(store, mark: .picture(Data(), scalePercent: 50)), Watermarking.noPicture)
        XCTAssertEqual(stamp(store, mark: .picture(Data([1]), scalePercent: 150)), "Size must be between 5 and 100 % of the page.")
        XCTAssertTrue(client.contentEdits.isEmpty)
    }

    func testATextStampCanGoBehindTheContent() {
        let (client, store) = opened()

        XCTAssertNil(stamp(store, layer: .underContent))

        guard case let .addWatermark(watermark) = client.contentEdits.first else { return XCTFail("nothing stamped") }
        XCTAssertEqual(watermark.layer, .underContent)
    }
}
