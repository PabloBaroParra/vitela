import XCTest
@testable import Vitela

final class ViewerStoreOutputSnapshotTests: XCTestCase {
    private let rect = PdfRect(x: 10, y: 10, width: 50, height: 50)

    func testAnUnannotatedSessionRendersFromItsOwnDocument() throws {
        let (client, store) = opened()

        let source = try XCTUnwrap(store.beginOutput())
        XCTAssertFalse(source.annotated)
        let output = try store.outputDocument(for: source).get()

        XCTAssertFalse(output is FakeSnapshotDocument)
        XCTAssertEqual(client.snapshotsTaken, 0, "nothing is missing, so there is no save that could fail")
    }

    func testAnAnnotatedSessionRendersFromASnapshot() throws {
        let (client, store) = opened()
        store.applyAnnotationEdit(.add(kind: .shape, page: 0, rect: rect, color: .annotationDefault))

        let source = try XCTUnwrap(store.beginOutput())
        XCTAssertTrue(source.annotated)
        let output = try store.outputDocument(for: source).get()

        XCTAssertTrue(output is FakeSnapshotDocument)
        XCTAssertEqual(client.snapshotsTaken, 1)
        XCTAssertEqual(store.annotations.count, 1, "the live session keeps its editable annotations")
    }

    func testUndoingTheOnlyAnnotationGoesBackToTheDocumentItself() throws {
        let (client, store) = opened()
        store.applyAnnotationEdit(.add(kind: .shape, page: 0, rect: rect, color: .annotationDefault))
        store.undo()

        _ = try store.outputDocument(for: try XCTUnwrap(store.beginOutput())).get()

        XCTAssertEqual(client.snapshotsTaken, 0)
    }

    func testPrintingAnAnnotatedSessionDrawsItsSnapshot() throws {
        let (client, store) = opened()
        store.applyAnnotationEdit(.add(kind: .shape, page: 0, rect: rect, color: .annotationDefault))

        let request = try XCTUnwrap(store.beginPrint())
        _ = store.printPage(0, for: request, dpi: ViewerStore.printPreviewDpi)

        XCTAssertTrue(request.document is FakeSnapshotDocument)
        XCTAssertEqual(client.renderedFromSnapshot, 1)
    }

    func testAPrintWhoseSnapshotFailsSaysWhyAndPrintsNothing() {
        let (client, store) = opened()
        client.snapshotError = EditFailure.failed("the save was refused")
        store.applyAnnotationEdit(.add(kind: .shape, page: 0, rect: rect, color: .annotationDefault))

        XCTAssertNil(store.beginPrint())
        XCTAssertTrue(store.editStatus.contains("the save was refused"), store.editStatus)
    }

    private func opened() -> (FakePdfCoreClient, ViewerStore) {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return (client, store)
    }
}
