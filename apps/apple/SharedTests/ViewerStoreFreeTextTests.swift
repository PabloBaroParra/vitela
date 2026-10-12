import XCTest
@testable import Vitela

final class ViewerStoreFreeTextTests: XCTestCase {
    private let a4 = PageDimensions(width: 595, height: 842)

    private func openStore(_ client: FakePdfCoreClient) -> ViewerStore {
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return store
    }

    // Windows' FreeTextInputTests, case for case.
    func testDialogRules() {
        XCTAssertFalse(ViewerStore.freeTextAddEnabled(""))
        XCTAssertFalse(ViewerStore.freeTextAddEnabled(" \r\n\t "))
        XCTAssertTrue(ViewerStore.freeTextAddEnabled("  hola  "))
        XCTAssertFalse(ViewerStore.freeTextEditChangesText(current: "uno", typed: "uno"))
        XCTAssertTrue(ViewerStore.freeTextEditChangesText(current: "uno", typed: "dos"))
        XCTAssertFalse(ViewerStore.freeTextEditChangesText(current: "uno", typed: "   "))
        XCTAssertTrue(ViewerStore.freeTextEditChangesText(current: "uno", typed: "uno "))
    }

    func testPlacement() {
        let tap = ViewerStore.freeTextPlacementRect(origin: PdfPoint(x: 100, y: 700), release: PdfPoint(x: 100, y: 700), pageWidth: 595, pageHeight: 842)
        XCTAssertEqual(tap, PdfRect(x: 100, y: 650, width: 200, height: 50), "a tap hangs the default box from the click")
        let rightEdge = ViewerStore.freeTextPlacementRect(origin: PdfPoint(x: 550, y: 700), release: PdfPoint(x: 550, y: 700), pageWidth: 595, pageHeight: 842)
        XCTAssertEqual(rightEdge, PdfRect(x: 395, y: 650, width: 200, height: 50), "the box slides left to stay on the page")
        let drag = ViewerStore.freeTextPlacementRect(origin: PdfPoint(x: 100, y: 700), release: PdfPoint(x: 300, y: 600), pageWidth: 595, pageHeight: 842)
        XCTAssertEqual(drag, PdfRect(x: 100, y: 600, width: 200, height: 100))
        let thin = ViewerStore.freeTextPlacementRect(origin: PdfPoint(x: 100, y: 700), release: PdfPoint(x: 110, y: 690), pageWidth: 595, pageHeight: 842)
        XCTAssertEqual(thin, PdfRect(x: 100, y: 682, width: 16, height: 18), "a small drag is raised to the minimum")
    }

    func testResizeKeepsTheMinimumAndGrowsAwayFromTheAnchor() {
        XCTAssertEqual(
            ViewerStore.freeTextResizedRect(anchor: PdfPoint(x: 100, y: 100), pointer: PdfPoint(x: 101, y: 99)),
            PdfRect(x: 100, y: 82, width: 16, height: 18)
        )
    }

    func testQuarterTurnedPagesSwapTheirUnrotatedSize() {
        XCTAssertTrue(ViewerStore.unrotatedSize(PageDimensions(width: 842, height: 595, rotation: .clockwise90)) == (595, 842))
        XCTAssertTrue(ViewerStore.unrotatedSize(a4) == (595, 842))
    }

    func testAClickPlacesABoxThatAsksForTextAndLandsSelected() throws {
        let client = FakePdfCoreClient(pages: [a4])
        let store = openStore(client)
        store.armAnnotationTool(.freeText)
        XCTAssertEqual(store.editStatus, "Text box armed. Click or drag on a page to place it.")

        XCTAssertTrue(store.beginAnnotationDraft(page: 0, at: PdfPoint(x: 100, y: 700)))
        store.endAnnotationDraft(page: 0, at: PdfPoint(x: 100, y: 700))
        let pending = try XCTUnwrap(store.pendingFreeText)
        XCTAssertEqual(pending.title, "Add text box — page 1")
        XCTAssertTrue(client.annotationEdits.isEmpty, "nothing is recorded before the text")

        XCTAssertNil(store.commitPendingFreeText("Hola"))

        XCTAssertEqual(client.annotationEdits, [.addFreeText(page: 0, rect: PdfRect(x: 100, y: 650, width: 200, height: 50), contents: "Hola")])
        XCTAssertEqual(store.selectedAnnotation?.kind, .freeText)
        XCTAssertEqual(store.editStatus, "Text box added. Changes are pending save.")
        XCTAssertNil(store.pendingFreeText)
    }

    func testARefusalKeepsTheSheetAndTheText() {
        let client = FakePdfCoreClient(pages: [a4])
        let store = openStore(client)
        store.beginFreeTextPlacement(page: 0, origin: PdfPoint(x: 10, y: 10), release: PdfPoint(x: 10, y: 10))
        client.annotationEditError = EditFailure.failed("This text's font cannot show \"字\". Try different characters.")

        XCTAssertEqual(store.commitPendingFreeText("字"), "This text's font cannot show \"字\". Try different characters.")
        XCTAssertNotNil(store.pendingFreeText)
    }

    func testEditingTheSelectedBoxChangesOnlyRealEdits() {
        let client = FakePdfCoreClient(pages: [a4])
        let store = openStore(client)
        store.beginFreeTextPlacement(page: 0, origin: PdfPoint(x: 10, y: 10), release: PdfPoint(x: 10, y: 10))
        _ = store.commitPendingFreeText("uno")
        let id = store.selectedAnnotationId!
        client.annotationEdits = []

        store.beginFreeTextEdit()
        XCTAssertEqual(store.pendingFreeText?.title, "Edit text box — page 1")
        XCTAssertNil(store.commitPendingFreeText("uno"))
        XCTAssertTrue(client.annotationEdits.isEmpty, "the same text is not an edit")

        store.beginFreeTextEdit()
        XCTAssertNil(store.commitPendingFreeText("dos"))
        XCTAssertEqual(client.annotationEdits, [.setContents(id: id, contents: "dos")])
        XCTAssertEqual(store.editStatus, "Text box edited. Changes are pending save.")
    }

    func testCancellingSaysWhichDialogWasCancelled() {
        let store = openStore(FakePdfCoreClient(pages: [a4]))
        store.beginFreeTextPlacement(page: 0, origin: PdfPoint(x: 10, y: 10), release: PdfPoint(x: 10, y: 10))

        store.cancelPendingFreeText()

        XCTAssertEqual(store.editStatus, "Text box placement canceled.")
        XCTAssertNil(store.pendingFreeText)
    }
}
