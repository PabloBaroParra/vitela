import XCTest
@testable import Vitela

/// Annotations, comments and page placement against the real core.
final class UniFfiAnnotationIntegrationTests: XCTestCase {
    func testAPlacedHighlightIsListedSavedAndUndoable() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        XCTAssertTrue(store.annotationEditingAllowed)
        store.armAnnotationTool(.highlight)

        XCTAssertTrue(store.beginAnnotationDraft(page: 0, at: PdfPoint(x: 72, y: 700)))
        store.endAnnotationDraft(page: 0, at: PdfPoint(x: 272, y: 680))

        let added = try XCTUnwrap(store.annotations.first)
        XCTAssertEqual(added.kind, .highlight)
        XCTAssertEqual(added.page, 0)
        XCTAssertEqual(added.rect, PdfRect(x: 72, y: 680, width: 200, height: 20))

        let save = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        let saved = try store.saveResult(for: save).get()
        XCTAssertNotNil(saved.range(of: Data("/Highlight".utf8)), "the saved file carries the annotation")

        store.undo()
        XCTAssertTrue(store.annotations.isEmpty)
    } }

    func testPlacementOnAQuarterTurnedPageSwapsTheBox() throws { try offMain {
        let client = UniFfiPdfCoreClient()
        let upright = PagePlacement(width: 612, height: 792, rotation: .none, scale: 1)
        let turned = PagePlacement(width: 792, height: 612, rotation: .clockwise90, scale: 1)
        let rect = PdfRect(x: 10, y: 20, width: 100, height: 2)

        XCTAssertEqual(client.placeRect(rect, on: upright), PlacedRect(left: 10, top: 770, width: 100, height: 2))
        let placed = client.placeRect(rect, on: turned)
        XCTAssertEqual(placed.width, 2)
        XCTAssertEqual(placed.height, 100)
        let back = client.pointToPdf(x: placed.left + 1, y: placed.top + 50, on: turned)
        XCTAssertTrue(rect.contains(back) || abs(back.x - 60) < 1.5, "a point inside the placed box maps back into the rect")
    } }
}
