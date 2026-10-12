import XCTest
@testable import Vitela

final class ViewerStoreFormPlacementTests: XCTestCase {
    private let a4 = PageDimensions(width: 595, height: 842)

    func testAClickHangsTheKindsBoxFromThePressAndADragPlacesWhatItDrew() {
        XCTAssertEqual(
            ViewerStore.fieldPlacementRect(origin: PdfPoint(x: 100, y: 700), end: PdfPoint(x: 103, y: 702), page: a4, clickSize: FieldToPlace.text.clickSize),
            PdfRect(x: 100, y: 664, width: 144, height: 36)
        )
        XCTAssertEqual(
            ViewerStore.fieldPlacementRect(origin: PdfPoint(x: 580, y: 10), end: PdfPoint(x: 580, y: 10), page: a4, clickSize: FieldToPlace.checkbox.clickSize),
            PdfRect(x: 577, y: 0, width: 18, height: 18),
            "the click box is kept on the page"
        )
        XCTAssertEqual(
            ViewerStore.fieldPlacementRect(origin: PdfPoint(x: 100, y: 700), end: PdfPoint(x: 300, y: 650), page: a4, clickSize: FieldToPlace.text.clickSize),
            PdfRect(x: 100, y: 650, width: 200, height: 50)
        )
        XCTAssertEqual(
            ViewerStore.fieldPlacementRect(origin: PdfPoint(x: 100, y: 700), end: PdfPoint(x: 900, y: 650), page: a4, clickSize: FieldToPlace.text.clickSize),
            PdfRect(x: 100, y: 650, width: 495, height: 50),
            "a drag off the page stops at its edge"
        )
    }

    func testPlacementUsesTheUnturnedPage() {
        let turned = PageDimensions(width: 842, height: 595, rotation: .clockwise90)
        let rect = ViewerStore.fieldPlacementRect(origin: PdfPoint(x: 500, y: 800), end: PdfPoint(x: 500, y: 800), page: turned, clickSize: FieldToPlace.text.clickSize)
        XCTAssertEqual(rect, PdfRect(x: 451, y: 764, width: 144, height: 36))
    }

    func testPlacingAFieldIsPageWorkThatListsTheNewField() throws {
        let client = FakePdfCoreClient(pages: [a4])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        store.armField(.dropdown)
        XCTAssertEqual(store.formStatus, "Click or drag on a page to place a dropdown.")

        XCTAssertTrue(store.beginFieldDraft(page: 0, at: PdfPoint(x: 100, y: 700)))
        let request = try XCTUnwrap(store.endFieldDraft(page: 0, at: PdfPoint(x: 100, y: 700)))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertEqual(client.addedFields, [.dropdown])
        XCTAssertEqual(client.refreshPreviewCount, 1, "the field's appearance is painted by the PDF")
        XCTAssertEqual(store.formFields.count, 1)
        XCTAssertNil(store.armedField, "the kind disarms after a placement")
        XCTAssertEqual(store.editStatus, "Dropdown placed. Changes are pending save.")
    }

    func testArmingAFieldAndAnAnnotationToolAreExclusive() {
        let store = ViewerStore(client: FakePdfCoreClient(pages: [a4]))
        store.open(bytes: Data([1]))

        store.armAnnotationTool(.highlight)
        store.armField(.text)
        XCTAssertNil(store.armedTool)

        store.armAnnotationTool(.shape)
        XCTAssertNil(store.armedField)
    }

    func testADocumentThatRefusesStructureChangesCannotArmAField() {
        let client = FakePdfCoreClient(pages: [a4])
        client.fieldStructurePermitted = false
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        store.armField(.checkbox)

        XCTAssertNil(store.armedField)
        XCTAssertEqual(store.formStatus, "This document does not permit creating form fields.")
    }
}
