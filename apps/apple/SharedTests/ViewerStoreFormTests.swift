import XCTest
@testable import Vitela

final class ViewerStoreFormTests: XCTestCase {
    private let rect = PdfRect(x: 10, y: 10, width: 100, height: 20)

    private func formStore() -> (FakePdfCoreClient, ViewerStore) {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.fields = [
            FormField(id: 1, page: 0, name: "Name", rect: rect, kind: .text(multiline: false, maxLength: nil), value: .text("")),
            FormField(id: 2, page: 0, name: "Agree", rect: rect, kind: .checkbox, value: .checked(false)),
        ]
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return (client, store)
    }

    private func run(_ store: ViewerStore, _ request: ViewerStore.PageWorkRequest?) throws {
        let request = try XCTUnwrap(request)
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
    }

    func testFieldsAreListedOnOpen() {
        let (_, store) = formStore()

        XCTAssertEqual(store.formFields.map(\.name), ["Name", "Agree"])
        XCTAssertTrue(store.formFillAllowed)
    }

    func testAFillIsOneUndoableEditThatRefreshesThePreview() throws {
        let (client, store) = formStore()

        try run(store, store.beginFieldFill(1, value: .text("Ada")))

        XCTAssertEqual(client.fills.map(\.value), [.text("Ada")])
        XCTAssertEqual(client.refreshPreviewCount, 1, "the PDF paints the value, so the render side is rebuilt")
        XCTAssertEqual(store.formFields.first?.value, .text("Ada"))
        XCTAssertEqual(store.editStatus, "Field filled in. Changes are pending save.")

        store.undo()
        try run(store, store.beginPageRefresh())
        XCTAssertEqual(store.formFields.first?.value, .text(""))
    }

    func testTheValueTheFieldAlreadyHoldsIsNotAnEdit() {
        let (client, store) = formStore()

        XCTAssertNil(store.beginFieldFill(2, value: .checked(false)))
        XCTAssertTrue(client.fills.isEmpty)
        XCTAssertFalse(store.pageWorkInFlight)
    }

    func testADocumentThatRefusesAnnotationsRefusesFilling() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.annotationsPermitted = false
        client.fields = [FormField(id: 1, page: 0, name: "Name", rect: rect, kind: .text(multiline: false, maxLength: nil), value: .text(""))]
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        XCTAssertEqual(store.formStatus, "This document does not permit filling in its form.")
        XCTAssertNil(store.beginFieldFill(1, value: .text("x")))
        XCTAssertTrue(client.fills.isEmpty)
    }

    func testARefusedFillPutsTheCoresValueBack() throws {
        let (client, store) = formStore()
        client.fieldFillError = EditFailure.failed("Value too long.")

        try run(store, store.beginFieldFill(1, value: .text("Ada")))

        XCTAssertEqual(store.editStatus, "Value too long.")
        XCTAssertEqual(store.formFields.first?.value, .text(""))
        XCTAssertFalse(store.hasUnsavedChanges)
    }

    func testChoicesAndTextHelpers() {
        XCTAssertEqual(FormFieldChoices.index(of: "b", in: ["a", "b"]), 2)
        XCTAssertEqual(FormFieldChoices.index(of: "z", in: ["a", "b"]), 0, "a value outside the options shows as none")
        XCTAssertNil(FormFieldChoices.choice(at: 0, in: ["a"]))
        XCTAssertEqual(FormFieldChoices.choice(at: 1, in: ["a"]), "a")
        XCTAssertEqual(FormFieldText.fromControl("a\r\nb\rc"), "a\nb\nc")
        XCTAssertEqual(FormFieldText.capped("abcdef", maxLength: 3), "abc")
        XCTAssertEqual(FormFieldText.capped("abc", maxLength: nil), "abc")
    }
}
