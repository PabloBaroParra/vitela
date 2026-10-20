import XCTest
@testable import Vitela

/// Typing the selected annotation's position and size — Windows' Move and
/// Resize dialogs over the core's move and resize.
final class ViewerStoreAnnotationGeometryTests: XCTestCase {
    private let letter = PageDimensions(width: 612, height: 792)
    private let box = PdfRect(x: 100, y: 100, width: 100, height: 50)

    /// One annotation of `kind` at `box` on page 0, selected.
    private func selected(_ kind: AnnotationKind = .shape) -> (FakePdfCoreClient, ViewerStore) {
        let client = FakePdfCoreClient(pages: [letter])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        store.applyAnnotationEdit(.add(kind: kind, page: 0, rect: box, color: .annotationDefault))
        client.annotationEdits = []
        store.selectedAnnotationId = store.annotations.first?.id
        return (client, store)
    }

    func testAPositionMovesTheBottomLeftCornerThereAndKeepsTheSize() throws {
        let (client, store) = selected()
        let id = try XCTUnwrap(store.selectedAnnotationId)

        XCTAssertNil(store.moveSelectedAnnotation(toX: 72, y: 600))

        XCTAssertEqual(client.annotationEdits, [.move(id: id, dx: -28, dy: 500)])
        XCTAssertEqual(store.annotations.first?.rect, PdfRect(x: 72, y: 600, width: 100, height: 50))
        XCTAssertEqual(store.editStatus, "Annotation moved. Changes are pending save.")
        XCTAssertTrue(store.canUndo)
    }

    func testASizeKeepsTheOriginAndSetsBothSidesApart() throws {
        let (client, store) = selected()
        let id = try XCTUnwrap(store.selectedAnnotationId)

        XCTAssertNil(store.resizeSelectedAnnotation(width: 300, height: 20))

        XCTAssertEqual(client.annotationEdits, [.resize(id: id, rect: PdfRect(x: 100, y: 100, width: 300, height: 20))])
        XCTAssertEqual(store.editStatus, "Annotation resized. Changes are pending save.")
    }

    func testTheSameNumbersRecordNothingAndSaySo() {
        let (client, store) = selected()
        let revision = store.editRevision

        XCTAssertNil(store.moveSelectedAnnotation(toX: 100, y: 100))
        XCTAssertEqual(store.editStatus, ViewerStore.AnnotationGeometry.positionUnchanged)
        XCTAssertNil(store.resizeSelectedAnnotation(width: 100, height: 50))
        XCTAssertEqual(store.editStatus, ViewerStore.AnnotationGeometry.sizeUnchanged)

        XCTAssertTrue(client.annotationEdits.isEmpty)
        XCTAssertEqual(store.editRevision, revision)
    }

    func testNumbersThatCannotBeAPlaceOrASizeKeepTheSheetOpen() {
        let (client, store) = selected()

        XCTAssertEqual(store.moveSelectedAnnotation(toX: .infinity, y: 0), ViewerStore.AnnotationGeometry.invalidPosition)
        XCTAssertEqual(store.moveSelectedAnnotation(toX: 0, y: .nan), ViewerStore.AnnotationGeometry.invalidPosition)
        XCTAssertEqual(store.resizeSelectedAnnotation(width: 0, height: 10), ViewerStore.AnnotationGeometry.invalidSize)
        XCTAssertEqual(store.resizeSelectedAnnotation(width: 10, height: -1), ViewerStore.AnnotationGeometry.invalidSize)
        XCTAssertTrue(client.annotationEdits.isEmpty)
    }

    func testATextBoxKeepsTheFloorACornerDragKeeps() {
        let (client, store) = selected(.freeText)

        XCTAssertEqual(store.resizeSelectedAnnotation(width: 10, height: 50), "A text box must be at least 16 × 18 points.")
        XCTAssertTrue(client.annotationEdits.isEmpty)
    }

    func testTheCoresRefusalStaysInTheSheet() {
        let (client, store) = selected()
        client.annotationEditError = EditFailure.failed("Annotation edits are not permitted.")

        XCTAssertEqual(store.moveSelectedAnnotation(toX: 1, y: 1), "Annotation edits are not permitted.")
    }
}
