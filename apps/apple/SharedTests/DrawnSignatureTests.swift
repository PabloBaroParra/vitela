import XCTest
@testable import Vitela

final class DrawnSignatureTests: XCTestCase {
    func testABareClickIsNotInk() {
        XCTAssertFalse(DrawnSignature.hasInk([]))
        XCTAssertFalse(DrawnSignature.hasInk([[PadPoint(x: 1, y: 1)]]))
        XCTAssertTrue(DrawnSignature.hasInk([[PadPoint(x: 1, y: 1)], [PadPoint(x: 1, y: 1), PadPoint(x: 5, y: 5)]]))
        XCTAssertNil(DrawnSignature.frame([[PadPoint(x: 1, y: 1)]]))
    }

    func testTheFrameCropsToTheInkPlusHalfAStroke() {
        let frame = DrawnSignature.frame([[PadPoint(x: 10, y: 20), PadPoint(x: 110, y: 70)]], strokeWidth: 4)
        XCTAssertEqual(frame, SignatureFrame(left: 8, top: 18, width: 104, height: 54, scale: 1))
        XCTAssertEqual(frame?.pixelWidth, 104)
    }

    func testALargeSignatureShrinksToTheCapAndASmallOneIsNeverEnlarged() throws {
        let large = try XCTUnwrap(DrawnSignature.frame([[PadPoint(x: 0, y: 0), PadPoint(x: 2397, y: 0)]], strokeWidth: 3))
        XCTAssertEqual(large.pixelWidth, 1200)
        let small = try XCTUnwrap(DrawnSignature.frame([[PadPoint(x: 0, y: 0), PadPoint(x: 7, y: 0)]], strokeWidth: 3))
        XCTAssertEqual(small.scale, 1)
    }

    func testAnArmedSignatureIsPlacedWhereTheClickLandsAndOnlyOnce() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let png = Data([137, 80, 78, 71, 13, 10, 26, 10, 9])

        store.armDrawnSignature(png)
        XCTAssertEqual(store.armedTool, .stamp)
        XCTAssertEqual(store.editStatus, "Click a page to place your signature.")
        XCTAssertTrue(store.beginAnnotationDraft(page: 0, at: PdfPoint(x: 300, y: 200)))
        store.endAnnotationDraft(page: 0, at: PdfPoint(x: 300, y: 200))

        XCTAssertEqual(client.stampInserts.map(\.rect), [PdfRect(x: 300, y: 150, width: 100, height: 50)])
        XCTAssertNil(store.pendingStamp, "no image picker for a drawn signature")
        XCTAssertNil(store.armedStampImage)
    }

    func testChoosingAnotherToolDropsAnArmedSignature() {
        let store = ViewerStore(client: FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)]))
        store.open(bytes: Data([1]))
        store.armDrawnSignature(Data([137, 80, 78, 71, 13, 10, 26, 10]))

        store.armAnnotationTool(.stamp)

        XCTAssertNil(store.armedStampImage)
    }
}
