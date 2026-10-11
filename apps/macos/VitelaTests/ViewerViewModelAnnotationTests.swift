import XCTest
@testable import Vitela

final class ViewerViewModelAnnotationTests: XCTestCase {
    private let letter = PageDimensions(width: 612, height: 792)

    func testADragWithAToolArmedPlacesAnAnnotationInsteadOfSelecting() {
        let client = FakePdfCoreClient(pages: [letter])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))
        model.store.armAnnotationTool(.highlight)

        // View space at zoom 2, top-left origin: (20, 84) → page (10, 750).
        model.pageDragBegan(page: 0, location: CGPoint(x: 20, y: 84), dimensions: letter, zoom: 2)
        model.pageDragChanged(page: 0, location: CGPoint(x: 220, y: 284), dimensions: letter, zoom: 2)
        XCTAssertNotNil(model.store.annotationDraft)
        model.pageDragEnded(page: 0, location: CGPoint(x: 220, y: 284), zoom: 2)

        XCTAssertEqual(client.annotationEdits, [
            .add(kind: .highlight, page: 0, rect: PdfRect(x: 10, y: 650, width: 100, height: 100), color: .annotationDefault),
        ])
        XCTAssertTrue(client.pageCharactersRequests.isEmpty)
        XCTAssertNil(model.store.selection)
    }

    func testWithoutAToolTheDragSelectsText() {
        let client = FakePdfCoreClient(pages: [letter])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))

        model.pageDragBegan(page: 0, location: CGPoint(x: 20, y: 84), dimensions: letter, zoom: 2)
        model.pageDragEnded(page: 0, location: CGPoint(x: 20, y: 84), zoom: 2)

        XCTAssertTrue(client.annotationEdits.isEmpty)
    }
}
