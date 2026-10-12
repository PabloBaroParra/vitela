import XCTest
import AppKit
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

extension ViewerViewModelAnnotationTests {
    func testDraggingAPlacedAnnotationMovesItInsteadOfSelectingText() {
        let client = FakePdfCoreClient(pages: [letter])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))
        model.store.applyAnnotationEdit(.add(kind: .shape, page: 0, rect: PdfRect(x: 10, y: 650, width: 100, height: 100), color: .annotationDefault))

        // (40, 184) at zoom 2 is page (20, 700): inside the shape.
        model.pageDragBegan(page: 0, location: CGPoint(x: 40, y: 184), dimensions: letter, zoom: 2)
        model.pageDragChanged(page: 0, location: CGPoint(x: 60, y: 184), dimensions: letter, zoom: 2)
        model.pageDragEnded(page: 0, location: CGPoint(x: 60, y: 184), zoom: 2)

        XCTAssertEqual(client.annotationEdits.last, .move(id: model.store.selectedAnnotationId!, dx: 10, dy: 0))
        XCTAssertTrue(client.pageCharactersRequests.isEmpty)
    }

    func testOnlyAnUnmodifiedDeleteKeyDeletes() throws {
        func key(_ code: UInt16, _ flags: NSEvent.ModifierFlags = []) throws -> NSEvent {
            try XCTUnwrap(NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0, windowNumber: 0,
                context: nil, characters: "", charactersIgnoringModifiers: "", isARepeat: false, keyCode: code
            ))
        }
        XCTAssertTrue(ViewerViewModel.isDeleteKey(try key(51)))
        XCTAssertTrue(ViewerViewModel.isDeleteKey(try key(117)))
        XCTAssertFalse(ViewerViewModel.isDeleteKey(try key(51, .command)))
        XCTAssertFalse(ViewerViewModel.isDeleteKey(try key(0)))
    }

    func testColorsArePickedAsEightBitSRGB() throws {
        let picked = try XCTUnwrap(CGColor(srgbRed: 1, green: 0.5, blue: 0, alpha: 1) as CGColor?)
        XCTAssertEqual(AnnotationToolbar.pdfColor(picked), PdfColor(r: 255, g: 128, b: 0))
    }
}
