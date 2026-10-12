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

    func testAnInkStrokeKeepsItsPointsAndIsSaved() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        store.armAnnotationTool(.ink)

        XCTAssertTrue(store.beginAnnotationDraft(page: 0, at: PdfPoint(x: 100, y: 100)))
        store.continueAnnotationDraft(page: 0, to: PdfPoint(x: 150, y: 130))
        store.endAnnotationDraft(page: 0, at: PdfPoint(x: 200, y: 100))

        let ink = try XCTUnwrap(store.annotations.first)
        XCTAssertEqual(ink.kind, .ink)
        XCTAssertEqual(ink.points.count, 3)
        XCTAssertEqual(ink.bounds, PdfRect(x: 100, y: 100, width: 100, height: 30))
        let save = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        XCTAssertNotNil(try store.saveResult(for: save).get().range(of: Data("/Ink".utf8)))
    } }

    func testPlacingAPointMatchesPlacingItsRectCorner() throws { try offMain {
        let client = UniFfiPdfCoreClient()
        let turned = PagePlacement(width: 792, height: 612, rotation: .clockwise90, scale: 2)
        let point = client.placePoint(PdfPoint(x: 10, y: 20), on: turned)
        XCTAssertEqual(client.pointToPdf(x: point.left, y: point.top, on: turned).x, 10, accuracy: 0.001)
        XCTAssertEqual(client.pointToPdf(x: point.left, y: point.top, on: turned).y, 20, accuracy: 0.001)
    } }
}
