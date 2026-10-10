import XCTest
@testable import Vitela

/// Runs the editing path against the real core through the generated
/// bindings — the fakes elsewhere cannot catch a mapping mistake between
/// `DocumentInfo` and `FfiDocumentInfo`. The test host is the app bundle, so
/// its `Contents/Frameworks` carries both dylibs.
///
/// Each test body runs on a background-QoS queue while the main thread waits
/// on an expectation (which spins the run loop rather than blocking). Every
/// core call waits on PDFium's render thread; waiting on it from a thread of
/// higher QoS trips Xcode's Thread Performance Checker, and symbolicating its
/// report has been seen to hang the whole test run. The lowest QoS cannot be
/// inverted. The store has no observers here, so leaving the main thread
/// changes nothing it does.
final class UniFfiEditingIntegrationTests: XCTestCase {
    private func offMain(_ body: @escaping () throws -> Void) throws {
        let done = expectation(description: "test body finished")
        var failure: Error?
        DispatchQueue.global(qos: .background).async {
            do { try body() } catch { failure = error }
            done.fulfill()
        }
        wait(for: [done], timeout: 30)
        if let failure { throw failure }
    }

    private static func sampleBytes() throws -> Data {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "vitela-sample", withExtension: "pdf"))
        return try Data(contentsOf: url)
    }

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
