import XCTest
@testable import Vitela

/// Form filling against the real core, on the repository's AcroForm fixture.
final class UniFfiFormIntegrationTests: XCTestCase {
    func testFillingARealFormFieldIsSavedAndUndoable() throws { try offMain {
        // The repository's AcroForm interop fixture (tests/fixtures/README.md),
        // read from the checkout the test was built from.
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("tests/fixtures/forms/reportlab_acroform.pdf")
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Data(contentsOf: url))
        let field = try XCTUnwrap(store.formFields.first { if case .text = $0.kind { return true } else { return false } })

        let request = try XCTUnwrap(store.beginFieldFill(field.id, value: .text("Vitela")))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertEqual(store.formFields.first { $0.id == field.id }?.value, .text("Vitela"))
        XCTAssertEqual(store.editStatus, "Field filled in. Changes are pending save.")

        let save = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        let reopened = ViewerStore(client: UniFfiPdfCoreClient())
        reopened.open(bytes: try store.saveResult(for: save).get())
        XCTAssertEqual(reopened.formFields.first { $0.name == field.name }?.value, .text("Vitela"))

        store.undo()
        XCTAssertEqual(store.formFields.first { $0.id == field.id }?.value, field.value)
    } }

    func testAPlacedFieldIsListedFillableSavedAndUndoable() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let before = Set(store.formFields.map(\.id))
        XCTAssertTrue(store.formStructureAllowed)
        store.armField(.text)
        XCTAssertTrue(store.beginFieldDraft(page: 0, at: PdfPoint(x: 72, y: 700)))
        let place = try XCTUnwrap(store.endFieldDraft(page: 0, at: PdfPoint(x: 72, y: 700)))
        store.applyPageWork(store.pageWorkResult(for: place), for: place)

        let field = try XCTUnwrap(store.formFields.first { !before.contains($0.id) })
        XCTAssertEqual(field.rect, PdfRect(x: 72, y: 664, width: 144, height: 36))
        let fill = try XCTUnwrap(store.beginFieldFill(field.id, value: .text("Hola")))
        store.applyPageWork(store.pageWorkResult(for: fill), for: fill)

        let save = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        let reopened = ViewerStore(client: UniFfiPdfCoreClient())
        reopened.open(bytes: try store.saveResult(for: save).get())
        XCTAssertTrue(reopened.formFields.contains { $0.value == .text("Hola") })

        store.undo()
        store.undo()
        XCTAssertEqual(Set(store.formFields.map(\.id)), before, "placing the field is its own undo step")
    } }

    func testRenamingRestylingMovingAndDeletingAPlacedField() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let before = Set(store.formFields.map(\.id))
        store.armField(.text)
        XCTAssertTrue(store.beginFieldDraft(page: 0, at: PdfPoint(x: 72, y: 700)))
        let place = try XCTUnwrap(store.endFieldDraft(page: 0, at: PdfPoint(x: 72, y: 700)))
        store.applyPageWork(store.pageWorkResult(for: place), for: place)
        let field = try XCTUnwrap(store.formFields.first { !before.contains($0.id) })
        store.selectedFieldId = field.id

        func run(_ request: ViewerStore.PageWorkRequest?) throws {
            let request = try XCTUnwrap(request)
            store.applyPageWork(store.pageWorkResult(for: request), for: request)
        }
        try run(store.beginFieldRename("Surname"))
        var style = field.style
        style.font = .timesRoman
        style.sizePt = 14
        try run(store.beginFieldRestyle(style))
        try run(store.beginFieldGeometryEdit(x: 100, y: 600, width: field.rect.width, height: field.rect.height))

        let changed = try XCTUnwrap(store.formFields.first { $0.id == field.id })
        XCTAssertEqual(changed.name, "Surname")
        XCTAssertEqual(changed.style.font, .timesRoman)
        XCTAssertEqual(changed.style.sizePt, 14)
        XCTAssertEqual(changed.rect.x, 100)
        XCTAssertEqual(changed.rect.y, 600)

        try run(store.beginFieldDelete())
        XCTAssertFalse(store.formFields.contains { $0.id == field.id })
        store.undo()
        XCTAssertTrue(store.formFields.contains { $0.name == "Surname" }, "undo brings the field back")
    } }
}
