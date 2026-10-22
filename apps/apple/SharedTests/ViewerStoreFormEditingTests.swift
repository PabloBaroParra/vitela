import XCTest
@testable import Vitela

final class ViewerStoreFormEditingTests: XCTestCase {
    private let box = PdfRect(x: 100, y: 100, width: 144, height: 36)

    private func editingStore() -> (FakePdfCoreClient, ViewerStore) {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.fields = [FormField(id: 5, page: 0, name: "Name", rect: box, kind: .text(multiline: false, maxLength: nil), value: .text(""))]
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        store.setFormEditMode(true)
        return (client, store)
    }

    private func run(_ store: ViewerStore, _ request: ViewerStore.PageWorkRequest?) throws {
        let request = try XCTUnwrap(request)
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
    }

    func testEditModeClaimsPressesSelectsFieldsAndDeselectsOnEmptySpace() {
        let (_, store) = editingStore()

        XCTAssertTrue(store.beginFieldGesture(page: 0, at: PdfPoint(x: 150, y: 110), zoom: 1))
        XCTAssertNil(store.endFieldGesture(at: PdfPoint(x: 150, y: 110)), "a still click is no edit")
        XCTAssertEqual(store.selectedField?.name, "Name")

        XCTAssertTrue(store.beginFieldGesture(page: 0, at: PdfPoint(x: 500, y: 500), zoom: 1), "edit mode owns the press")
        XCTAssertNil(store.selectedFieldId)
    }

    func testFocusingAPanelControlSelectsItsFieldOnlyInEditMode() {
        let (_, store) = editingStore()
        store.selectFieldFromPanel(5)
        XCTAssertEqual(store.selectedFieldId, 5)

        store.selectFieldFromPanel(99)
        XCTAssertEqual(store.selectedFieldId, 5, "a field that is gone selects nothing")

        store.setFormEditMode(false)
        store.selectFieldFromPanel(5)
        XCTAssertNil(store.selectedFieldId, "just filling in values never selects")
    }

    func testDraggingMovesAndACornerResizesByThePointersDelta() throws {
        let (client, store) = editingStore()
        _ = store.beginFieldGesture(page: 0, at: PdfPoint(x: 150, y: 110), zoom: 1)
        try run(store, store.endFieldGesture(at: PdfPoint(x: 170, y: 130)))
        XCTAssertEqual(client.fieldEdits, [.move(id: 5, to: PdfRect(x: 120, y: 120, width: 144, height: 36))])
        XCTAssertEqual(store.editStatus, "Field moved. Changes are pending save.")

        // The top-right handle, grabbed 3 points off its corner: it moves by
        // the delta, so the off-corner grab does not snap the size.
        _ = store.beginFieldGesture(page: 0, at: PdfPoint(x: 261, y: 153), zoom: 1)
        XCTAssertEqual(store.fieldGesture?.corner, .topRight)
        try run(store, store.endFieldGesture(at: PdfPoint(x: 271, y: 163)))
        XCTAssertEqual(client.fieldEdits.last, .resize(id: 5, to: PdfRect(x: 120, y: 120, width: 154, height: 46)))
    }

    func testASettledTextColorLandsOnItsFieldOnceAndUnchangedIsNoEdit() throws {
        let (client, store) = editingStore()
        let blue = PdfColor(r: 0, g: 0, b: 255)
        XCTAssertNil(store.selectedFieldId, "not selected: the color still lands on its own field")

        try run(store, store.beginFieldColor(id: 5, color: blue))
        XCTAssertNil(store.beginFieldColor(id: 5, color: blue), "an unchanged color is no second undo step")

        var style = FormTextStyle.default
        style.color = blue
        XCTAssertEqual(client.fieldEdits, [.restyle(id: 5, style: style)])
    }

    func testTheInspectorRenamesRestylesPlacesAndDeletes() throws {
        let (client, store) = editingStore()
        store.selectedFieldId = 5

        XCTAssertNil(store.beginFieldRename("Name"), "the same name is no edit")
        try run(store, store.beginFieldRename("Full name"))
        var style = FormTextStyle.default
        style.font = .courier
        try run(store, store.beginFieldRestyle(style))
        try run(store, store.beginFieldGeometryEdit(x: 10, y: 20, width: 144, height: 36))
        XCTAssertNil(store.beginFieldGeometryEdit(x: .nan, y: 0, width: 1, height: 1))
        XCTAssertEqual(store.formStatus, "Enter finite field coordinates.")
        try run(store, store.beginFieldDelete())

        XCTAssertEqual(client.fieldEdits, [
            .rename(id: 5, name: "Full name"),
            .restyle(id: 5, style: style),
            .move(id: 5, to: PdfRect(x: 10, y: 20, width: 144, height: 36)),
            .remove(id: 5),
        ])
        XCTAssertNil(store.selectedFieldId)
        XCTAssertTrue(store.formFields.isEmpty)
    }

    func testEditModeAndAnnotationToolsAreExclusive() {
        let (_, store) = editingStore()
        store.selectedFieldId = 5

        store.armAnnotationTool(.ink)

        XCTAssertFalse(store.formEditMode)
        XCTAssertNil(store.selectedFieldId)
        XCTAssertFalse(store.beginFieldGesture(page: 0, at: PdfPoint(x: 150, y: 110), zoom: 1))
    }

    func testADocumentThatRefusesStructureChangesCannotEnterEditMode() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.fieldStructurePermitted = false
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        store.setFormEditMode(true)

        XCTAssertFalse(store.formEditMode)
    }
}
