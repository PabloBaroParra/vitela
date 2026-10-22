import XCTest
@testable import Vitela

final class ViewerStoreAnnotationEditingTests: XCTestCase {
    private let letter = PageDimensions(width: 612, height: 792)
    private let box = PdfRect(x: 100, y: 100, width: 100, height: 50)

    /// One shape at `box` on page 0, placed through the store.
    private func storeWithShape() -> (FakePdfCoreClient, ViewerStore) {
        let client = FakePdfCoreClient(pages: [letter, letter])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        store.applyAnnotationEdit(.add(kind: .shape, page: 0, rect: box, color: .annotationDefault))
        client.annotationEdits = []
        return (client, store)
    }

    func testAClickOnAnAnnotationSelectsItAndAClickElsewhereDeselects() {
        let (_, store) = storeWithShape()

        XCTAssertTrue(store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 150, y: 120), zoom: 1))
        store.endAnnotationGesture(at: PdfPoint(x: 150, y: 120))
        XCTAssertEqual(store.selectedAnnotation?.kind, .shape)

        XCTAssertFalse(store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 400, y: 400), zoom: 1), "empty space goes on to text selection")
        XCTAssertNil(store.selectedAnnotationId)
    }

    func testDraggingTheBodyMovesItByTheDragDistance() {
        let (client, store) = storeWithShape()

        XCTAssertTrue(store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 150, y: 120), zoom: 1))
        store.continueAnnotationGesture(to: PdfPoint(x: 170, y: 110))
        XCTAssertEqual(store.annotationGesture?.previewRect, PdfRect(x: 120, y: 90, width: 100, height: 50))
        store.endAnnotationGesture(at: PdfPoint(x: 170, y: 110))

        let id = store.selectedAnnotationId!
        XCTAssertEqual(client.annotationEdits, [.move(id: id, dx: 20, dy: -10)])
        XCTAssertEqual(store.annotations.first?.rect, PdfRect(x: 120, y: 90, width: 100, height: 50))
    }

    func testAClickWithoutMovementRecordsNothing() {
        let (client, store) = storeWithShape()
        let revision = store.editRevision

        XCTAssertTrue(store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 150, y: 120), zoom: 1))
        store.endAnnotationGesture(at: PdfPoint(x: 150, y: 120))

        XCTAssertTrue(client.annotationEdits.isEmpty)
        XCTAssertEqual(store.editRevision, revision, "no undo step for a click")
    }

    func testTheSelectedAnnotationsCornerResizesWithTheOppositeCornerFixed() {
        let (client, store) = storeWithShape()
        _ = store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 150, y: 120), zoom: 1)
        store.endAnnotationGesture(at: PdfPoint(x: 150, y: 120))
        let id = store.selectedAnnotationId!

        // The top-right handle, within its 8-point reach at zoom 1.
        XCTAssertTrue(store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 203, y: 148), zoom: 1))
        XCTAssertEqual(store.annotationGesture?.corner, .topRight)
        store.endAnnotationGesture(at: PdfPoint(x: 260, y: 200))

        XCTAssertEqual(client.annotationEdits, [.resize(id: id, rect: PdfRect(x: 100, y: 100, width: 160, height: 100))])
    }

    func testDraggingACornerPastItsOppositeFlipsTheRect() {
        XCTAssertEqual(
            ViewerStore.resizedRect(PdfRect(x: 100, y: 100, width: 100, height: 50), corner: .topRight, to: PdfPoint(x: 60, y: 80)),
            PdfRect(x: 60, y: 80, width: 40, height: 20)
        )
        XCTAssertEqual(
            ViewerStore.resizedRect(PdfRect(x: 100, y: 100, width: 100, height: 50), corner: .topRight, to: PdfPoint(x: 101, y: 101)),
            PdfRect(x: 100, y: 100, width: 4, height: 4),
            "never thinner than a traced rect"
        )
    }

    func testHandleReachIsInScreenPointsSoItShrinksInPageSpaceAtHighZoom() {
        XCTAssertEqual(ViewerStore.corner(of: box, at: PdfPoint(x: 205, y: 150), reach: 8 / 1), .topRight)
        XCTAssertNil(ViewerStore.corner(of: box, at: PdfPoint(x: 205, y: 150), reach: 8 / 4))
    }

    func testASettledColorLandsOnItsAnnotationEvenOnceDeselected() {
        let (client, store) = storeWithShape()
        _ = store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 150, y: 120), zoom: 1)
        store.endAnnotationGesture(at: PdfPoint(x: 150, y: 120))
        let id = store.selectedAnnotationId!
        store.selectedAnnotationId = nil

        store.restyleAnnotation(id: id, to: PdfColor(r: 0, g: 0, b: 255))
        store.restyleAnnotation(id: id, to: PdfColor(r: 0, g: 0, b: 255))

        XCTAssertEqual(client.annotationEdits.last, .restyle(id: id, color: PdfColor(r: 0, g: 0, b: 255)))
        XCTAssertEqual(client.annotationEdits.filter { if case .restyle = $0 { return true }; return false }.count, 1,
                       "an unchanged color is no second undo step")
    }

    func testDeleteNudgeGrowAndRestyleTheSelection() {
        let (client, store) = storeWithShape()
        _ = store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 150, y: 120), zoom: 1)
        store.endAnnotationGesture(at: PdfPoint(x: 150, y: 120))
        let id = store.selectedAnnotationId!

        store.nudgeSelectedAnnotation()
        store.growSelectedAnnotation()
        store.restyleSelectedAnnotation(PdfColor(r: 0, g: 0, b: 255))
        XCTAssertEqual(store.editStatus, "Annotation restyled. Changes are pending save.")
        store.deleteSelectedAnnotation()

        XCTAssertEqual(client.annotationEdits, [
            .move(id: id, dx: 12, dy: 12),
            .resize(id: id, rect: PdfRect(x: 112, y: 112, width: 125, height: 62.5)),
            .restyle(id: id, color: PdfColor(r: 0, g: 0, b: 255)),
            .remove(id: id),
        ])
        XCTAssertNil(store.selectedAnnotationId)
        XCTAssertTrue(store.annotations.isEmpty)
    }

    func testANoteCannotBeRestyledAndInkCannotGrow() {
        let client = FakePdfCoreClient(pages: [letter])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        store.applyAnnotationEdit(.addInk(page: 0, points: [PdfPoint(x: 10, y: 10), PdfPoint(x: 50, y: 50)], color: .annotationDefault))
        store.applyAnnotationEdit(.addNote(page: 0, rect: PdfRect(x: 300, y: 300, width: 20, height: 20), contents: "x"))
        client.annotationEdits = []

        _ = store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 30, y: 30), zoom: 1)
        store.endAnnotationGesture(at: PdfPoint(x: 30, y: 30))
        XCTAssertEqual(store.selectedAnnotation?.kind, .ink)
        store.growSelectedAnnotation()

        _ = store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 310, y: 310), zoom: 1)
        store.endAnnotationGesture(at: PdfPoint(x: 310, y: 310))
        XCTAssertEqual(store.selectedAnnotation?.kind, .textNote)
        store.restyleSelectedAnnotation(PdfColor(r: 1, g: 2, b: 3))

        XCTAssertTrue(client.annotationEdits.isEmpty)
    }

    func testUndoingTheSelectedAnnotationAwayClearsTheSelection() {
        let (_, store) = storeWithShape()
        _ = store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 150, y: 120), zoom: 1)
        store.endAnnotationGesture(at: PdfPoint(x: 150, y: 120))

        store.undo()

        XCTAssertTrue(store.annotations.isEmpty)
        XCTAssertNil(store.selectedAnnotationId)
    }

    func testArmingAToolClearsTheSelectionAndAnnotationsOnAnotherPageAreNotHit() {
        let (_, store) = storeWithShape()
        XCTAssertFalse(store.beginAnnotationGesture(page: 1, at: PdfPoint(x: 150, y: 120), zoom: 1))

        _ = store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 150, y: 120), zoom: 1)
        store.endAnnotationGesture(at: PdfPoint(x: 150, y: 120))
        store.armAnnotationTool(.ink)

        XCTAssertNil(store.selectedAnnotationId)
        XCTAssertFalse(store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 150, y: 120), zoom: 1), "an armed tool owns the press")
    }
}
