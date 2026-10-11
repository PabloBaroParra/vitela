import XCTest
@testable import Vitela

/// The metadata editing path against the real core — the fakes elsewhere
/// cannot catch a mapping mistake between `DocumentInfo` and `FfiDocumentInfo`.
final class UniFfiEditingIntegrationTests: XCTestCase {
    func testPropertiesSurviveASaveAndReopen() throws { try offMain {
        let client = UniFfiPdfCoreClient()
        let store = ViewerStore(client: client)
        store.open(bytes: try Self.sampleBytes())
        XCTAssertEqual(store.state, .loaded)
        XCTAssertTrue(store.editingAllowed)

        store.setDocumentProperty(.title, to: "Vitela integration")
        store.setDocumentProperty(.keywords, to: "ñandú, café")
        XCTAssertTrue(store.setDocumentDate(.created, to: "2021-06-15 08:30:00"))
        XCTAssertTrue(store.canUndo)

        let request = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        let bytes = try store.saveResult(for: request).get()

        let reopened = ViewerStore(client: client)
        reopened.open(bytes: bytes)
        XCTAssertEqual(reopened.documentInfo?.title, "Vitela integration")
        XCTAssertEqual(reopened.documentInfo?.keywords, "ñandú, café")
        XCTAssertEqual(MetadataDateText.format(reopened.documentInfo?.creationDate), "2021-06-15 08:30:00")
    } }

    func testUndoRestoresTheFilesOwnValue() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let original = store.documentInfo

        store.setDocumentProperty(.author, to: "Someone else")
        store.undo()

        XCTAssertEqual(store.documentInfo, original)
        XCTAssertFalse(store.canUndo)
        XCTAssertTrue(store.canRedo)
    } }
}
