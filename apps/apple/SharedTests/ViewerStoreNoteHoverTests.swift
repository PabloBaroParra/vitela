import XCTest
@testable import Vitela

final class ViewerStoreNoteHoverTests: XCTestCase {
    private func store() -> ViewerStore {
        let store = ViewerStore(client: FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)]))
        store.open(bytes: Data([1]))
        return store
    }

    func testANoteSaysWhatItHoldsWhereThePointerRests() {
        let store = store()
        store.applyAnnotationEdit(.addNote(page: 0, rect: PdfRect(x: 100, y: 100, width: 24, height: 24), contents: "  Check the total \n"))

        XCTAssertEqual(store.noteText(page: 0, at: PdfPoint(x: 110, y: 110)), "Check the total")
        XCTAssertNil(store.noteText(page: 0, at: PdfPoint(x: 300, y: 300)))
    }

    func testTheTopmostAnnotationWinsAndDoesNotReachThroughToANoteBelow() {
        let store = store()
        store.applyAnnotationEdit(.addNote(page: 0, rect: PdfRect(x: 100, y: 100, width: 24, height: 24), contents: "Hidden"))
        store.applyAnnotationEdit(.add(kind: .shape, page: 0, rect: PdfRect(x: 90, y: 90, width: 60, height: 60), color: .annotationDefault))

        XCTAssertNil(store.noteText(page: 0, at: PdfPoint(x: 110, y: 110)))
    }

    func testAnEmptyNoteHasNoTooltip() {
        let store = store()
        store.applyAnnotationEdit(.addNote(page: 0, rect: PdfRect(x: 100, y: 100, width: 24, height: 24), contents: "   "))

        XCTAssertNil(store.noteText(page: 0, at: PdfPoint(x: 110, y: 110)))
        XCTAssertTrue(store.noteTooltips(page: 0, zoom: 1).isEmpty)
    }

    func testEachNoteIsPlacedOnTheDrawnPageAtTheZoom() {
        let store = store()
        store.applyAnnotationEdit(.addNote(page: 0, rect: PdfRect(x: 100, y: 692, width: 24, height: 20), contents: "Top note"))
        store.applyAnnotationEdit(.add(kind: .highlight, page: 0, rect: PdfRect(x: 10, y: 10, width: 50, height: 10), color: .annotationDefault))

        let tooltips = store.noteTooltips(page: 0, zoom: 2)

        XCTAssertEqual(tooltips, [NoteTooltip(frame: PlacedRect(left: 200, top: 160, width: 48, height: 40), text: "Top note")])
    }

    func testAVeryLongNoteIsCutForTheTooltip() {
        let text = String(repeating: "a", count: ViewerStore.noteTooltipMaxLength + 10)
        XCTAssertEqual(ViewerStore.tooltipText(text)?.count, ViewerStore.noteTooltipMaxLength + 1)
        XCTAssertNil(ViewerStore.tooltipText(nil))
    }
}
