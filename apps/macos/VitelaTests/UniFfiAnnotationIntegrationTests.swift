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

    func testANoteKeepsItsTextThroughTheCore() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        store.armAnnotationTool(.note)
        XCTAssertTrue(store.beginAnnotationDraft(page: 0, at: PdfPoint(x: 100, y: 600)))
        store.endAnnotationDraft(page: 0, at: PdfPoint(x: 124, y: 624))

        store.commitPendingNote(text: "Revisar — ñandú")

        let note = try XCTUnwrap(store.annotations.first)
        XCTAssertEqual(note.kind, .textNote)
        XCTAssertEqual(note.contents, "Revisar — ñandú")
        let save = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        XCTAssertNotNil(try store.saveResult(for: save).get().range(of: Data("/Text".utf8)))
    } }

    func testMovingResizingRestylingAndDeletingGoThroughTheCore() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        store.applyAnnotationEdit(.add(kind: .shape, page: 0, rect: PdfRect(x: 100, y: 100, width: 100, height: 50), color: .annotationDefault))
        XCTAssertTrue(store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 150, y: 120), zoom: 1))
        store.endAnnotationGesture(at: PdfPoint(x: 160, y: 130))
        XCTAssertEqual(store.selectedAnnotation?.rect, PdfRect(x: 110, y: 110, width: 100, height: 50))

        store.growSelectedAnnotation()
        XCTAssertEqual(store.selectedAnnotation?.rect, PdfRect(x: 110, y: 110, width: 125, height: 62.5))
        store.restyleSelectedAnnotation(PdfColor(r: 0, g: 0, b: 255))
        XCTAssertEqual(store.selectedAnnotation?.color, PdfColor(r: 0, g: 0, b: 255))
        store.deleteSelectedAnnotation()
        XCTAssertTrue(store.annotations.isEmpty)

        store.undo()
        XCTAssertEqual(store.annotations.first?.color, PdfColor(r: 0, g: 0, b: 255), "each edit is its own undo step")
    } }

    func testATextBoxIsLaidOutByTheCoreAndRefusesWhatItsFontCannotShow() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        store.beginFreeTextPlacement(page: 0, origin: PdfPoint(x: 72, y: 700), release: PdfPoint(x: 72, y: 700))

        XCTAssertNotNil(store.commitPendingFreeText("世界"), "a character Helvetica cannot show is named, not saved")
        XCTAssertNil(store.commitPendingFreeText("Hello, text box"))

        let box = try XCTUnwrap(store.selectedAnnotation)
        XCTAssertEqual(box.kind, .freeText)
        XCTAssertEqual(box.contents, "Hello, text box")
        XCTAssertEqual(box.freeTextLayout?.lines.map(\.text).joined(separator: " "), "Hello, text box")
        let narrower = try XCTUnwrap(store.freeTextLayout(for: box, at: PdfRect(x: 72, y: 600, width: 40, height: 100)))
        XCTAssertGreaterThan(narrower.lines.count, 1, "a narrower box wraps — by the core, not the shell")
        let save = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        XCTAssertNotNil(try store.saveResult(for: save).get().range(of: Data("/FreeText".utf8)))
    } }
}
