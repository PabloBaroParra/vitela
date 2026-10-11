import XCTest
@testable import Vitela

final class ViewerStoreAnnotationTests: XCTestCase {
    private let letter = PageDimensions(width: 612, height: 792)

    private func openStore(_ client: FakePdfCoreClient) -> ViewerStore {
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return store
    }

    private func place(_ store: ViewerStore, page: Int = 0, from: PdfPoint, to: PdfPoint) {
        XCTAssertTrue(store.beginAnnotationDraft(page: page, at: from))
        store.continueAnnotationDraft(page: page, to: to)
        store.endAnnotationDraft(page: page, at: to)
    }

    func testADragWithAnArmedToolPlacesOneAnnotationAndDisarms() {
        let client = FakePdfCoreClient(pages: [letter])
        let store = openStore(client)
        store.armAnnotationTool(.highlight)
        XCTAssertEqual(store.editStatus, "Highlight armed. Drag on a page to place it.")

        place(store, from: PdfPoint(x: 100, y: 500), to: PdfPoint(x: 40, y: 450))

        XCTAssertEqual(client.annotationEdits, [
            .add(kind: .highlight, page: 0, rect: PdfRect(x: 40, y: 450, width: 60, height: 50), color: .annotationDefault),
        ])
        XCTAssertEqual(store.annotations.count, 1)
        XCTAssertNil(store.armedTool)
        XCTAssertNil(store.annotationDraft)
        XCTAssertTrue(store.hasUnsavedChanges)
        XCTAssertEqual(store.editStatus, "Annotation edited. Changes are pending save.")
    }

    func testWithoutAnArmedToolTheDragIsNotClaimed() {
        let store = openStore(FakePdfCoreClient(pages: [letter]))

        XCTAssertFalse(store.beginAnnotationDraft(page: 0, at: PdfPoint(x: 1, y: 1)))
        XCTAssertNil(store.annotationDraft)
    }

    func testUnderlineAndStrikeoutAreRulesOnTheStartingLine() {
        XCTAssertEqual(
            ViewerStore.tracedRect(from: PdfPoint(x: 50, y: 300), to: PdfPoint(x: 150, y: 280), tool: .underline),
            PdfRect(x: 50, y: 300, width: 100, height: 2)
        )
        XCTAssertEqual(
            ViewerStore.tracedRect(from: PdfPoint(x: 50, y: 300), to: PdfPoint(x: 51, y: 301), tool: .shape),
            PdfRect(x: 50, y: 300, width: 4, height: 4),
            "a click still leaves a rect the reader can see and grab"
        )
    }

    func testUndoAndRedoReReadTheCoresList() {
        let client = FakePdfCoreClient(pages: [letter])
        let store = openStore(client)
        store.armAnnotationTool(.shape)
        place(store, from: PdfPoint(x: 10, y: 10), to: PdfPoint(x: 90, y: 90))

        store.undo()
        XCTAssertTrue(store.annotations.isEmpty)
        store.redo()
        XCTAssertEqual(store.annotations.map(\.kind), [.shape])
    }

    func testADocumentThatRefusesAnnotationsCannotArmATool() {
        let client = FakePdfCoreClient(pages: [letter])
        client.annotationsPermitted = false
        let store = openStore(client)

        store.armAnnotationTool(.highlight)

        XCTAssertFalse(store.annotationEditingAllowed)
        XCTAssertNil(store.armedTool)
    }

    func testARefusedEditReportsTheReasonAndRecordsNothing() {
        let client = FakePdfCoreClient(pages: [letter])
        client.annotationEditError = EditFailure.notPermitted("Annotation editing is not permitted.")
        let store = openStore(client)
        store.armAnnotationTool(.highlight)

        place(store, from: PdfPoint(x: 10, y: 10), to: PdfPoint(x: 90, y: 90))

        XCTAssertEqual(store.editStatus, "Annotation editing is not permitted.")
        XCTAssertFalse(store.hasUnsavedChanges)
    }

    func testOpeningAnotherDocumentDropsToolsAndDrafts() {
        let store = openStore(FakePdfCoreClient(pages: [letter]))
        store.armAnnotationTool(.highlight)
        _ = store.beginAnnotationDraft(page: 0, at: PdfPoint(x: 1, y: 1))

        store.open(bytes: Data([2]))

        XCTAssertNil(store.armedTool)
        XCTAssertNil(store.annotationDraft)
    }

    func testAnnotationsAreListedByPage() {
        let client = FakePdfCoreClient(pages: [letter, letter])
        let store = openStore(client)
        store.armAnnotationTool(.highlight)
        place(store, page: 1, from: PdfPoint(x: 10, y: 10), to: PdfPoint(x: 90, y: 90))

        XCTAssertTrue(store.annotations(onPage: 0).isEmpty)
        XCTAssertEqual(store.annotations(onPage: 1).count, 1)
    }

    func testInkBoundsCoverItsPoints() {
        let ink = Annotation(
            id: 1, page: 0, kind: .ink, rect: nil,
            points: [PdfPoint(x: 5, y: 9), PdfPoint(x: 1, y: 20), PdfPoint(x: 8, y: 2)], color: nil, contents: nil
        )
        XCTAssertEqual(ink.bounds, PdfRect(x: 1, y: 2, width: 7, height: 18))
    }
}
