import XCTest
@testable import Vitela

final class ViewerViewModelTests: XCTestCase {
    func testEmptyAndErrorStatesExposeActionableText() throws {
        let model = ViewerViewModel(store: ViewerStore(client: EmptyClient()))

        XCTAssertEqual(model.title, "Open a PDF to begin")
        model.store.open(bytes: Data([0]))
        XCTAssertEqual(model.title, "Could not open PDF")
    }

    func testTitleTracksTheStateItWasNotifiedWithRatherThanTheOneItReadsBack() throws {
        let client = FakePagesClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))

        model.store.open(bytes: Data([1]))

        // `@Published` fires on `willSet`, so a sink that re-read `store.state`
        // would still see the previous value here.
        XCTAssertEqual(model.title, "PDF open")
    }

    func testSelectingADocumentRaisesTheImporterFlag() throws {
        let model = ViewerViewModel(store: ViewerStore(client: EmptyClient()))

        model.selectDocument()

        XCTAssertTrue(model.isSelectingDocument)
    }

    func testDismissingThePickerIsACancellationRatherThanAReadFailure() throws {
        let client = FakePagesClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))
        model.selectDocument()

        // iOS reports a dismissed importer as a *failure* carrying
        // NSUserCancelledError, not as an empty success.
        model.finishSelection(.failure(NSError(domain: NSCocoaErrorDomain, code: NSUserCancelledError)))

        XCTAssertFalse(model.isSelectingDocument)
        XCTAssertEqual(model.store.state, .loaded)
        XCTAssertEqual(model.title, "PDF open")
    }

    func testASelectionWithNoUrlIsACancellationAndKeepsTheOpenDocument() throws {
        let client = FakePagesClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))
        model.selectDocument()

        model.finishSelection(.success([]))

        XCTAssertFalse(model.isSelectingDocument)
        XCTAssertEqual(model.store.state, .loaded)
        XCTAssertEqual(model.store.pageSlots.count, 1)
    }

    func testARealImporterFailureSurfacesAsAReadFailure() throws {
        let model = ViewerViewModel(store: ViewerStore(client: EmptyClient()))
        model.selectDocument()

        model.finishSelection(.failure(NSError(
            domain: NSCocoaErrorDomain,
            code: NSFileReadNoPermissionError,
            userInfo: [NSLocalizedDescriptionKey: "permission denied"]
        )))

        XCTAssertFalse(model.isSelectingDocument)
        XCTAssertEqual(model.store.state, .error(.readFailed("permission denied")))
    }

    func testUnreadableUrlIsReportedAsAReadFailureAndKeepsTheOpenDocument() throws {
        let client = FakePagesClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))

        // A path the sandbox cannot read: `open(url:)` runs on its own queue, so
        // the assertion has to wait for the failure to land back on main.
        let missing = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("vitela-does-not-exist-\(UUID().uuidString).pdf")
        model.open(url: missing)

        let errored = expectation(description: "read failure reaches the store")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { errored.fulfill() }
        wait(for: [errored], timeout: 2)

        guard case .error(.readFailed) = model.store.state else {
            return XCTFail("expected a read failure, got \(model.store.state)")
        }
        // The failed open must not have closed the document already on screen.
        XCTAssertEqual(model.store.pageSlots.count, 1)
    }
}

private struct EmptyClient: PdfCoreClient {
    func open(bytes: Data) throws -> any PdfDocument {
        throw ViewerFailure.openFailed("invalid PDF")
    }

    func render(document: any PdfDocument, page: Int, dpi: Int) throws -> RenderedPage {
        throw ViewerFailure.renderFailed(page: page, message: "not available")
    }
}

private struct FakePagesClient: PdfCoreClient {
    let pages: [PageDimensions]

    func open(bytes: Data) throws -> any PdfDocument {
        FakeDocument(pages: pages)
    }

    func render(document: any PdfDocument, page: Int, dpi: Int) throws -> RenderedPage {
        RenderedPage.placeholder
    }
}

private struct FakeDocument: PdfDocument {
    let pages: [PageDimensions]
}
