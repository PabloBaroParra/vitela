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
}
