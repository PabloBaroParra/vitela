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

    func testUnreadableFileSurfacesAsAnErrorTitle() throws {
        let client = FakePagesClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))

        model.store.reportOpenFailure(.readFailed("permission denied"))

        XCTAssertEqual(model.title, "Could not open PDF")
    }

    func testSubmitPasswordReopensTheSameDocumentAndSucceedsWithTheRightOne() throws {
        let client = PasswordGatedClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))
        XCTAssertEqual(model.store.state, .error(.passwordRequired))

        model.submitPassword("right-password")

        XCTAssertEqual(model.store.state, .loaded)
        XCTAssertEqual(model.title, "PDF open")
    }

    func testSubmitPasswordWithTheWrongValueReportsWrongPasswordRatherThanReprompting() throws {
        let client = PasswordGatedClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))

        model.submitPassword("guess")

        XCTAssertEqual(model.store.state, .error(.wrongPassword))
    }
}

private struct EmptyClient: PdfCoreClient {
    func open(bytes: Data, password: String?) throws -> any PdfDocument {
        throw ViewerFailure.openFailed("invalid PDF")
    }

    func render(document: any PdfDocument, page: Int, dpi: Int) throws -> RenderedPage {
        throw ViewerFailure.renderFailed(page: page, message: "not available")
    }
}

private struct FakePagesClient: PdfCoreClient {
    let pages: [PageDimensions]

    func open(bytes: Data, password: String?) throws -> any PdfDocument {
        FakeDocument(pages: pages)
    }

    func render(document: any PdfDocument, page: Int, dpi: Int) throws -> RenderedPage {
        RenderedPage.placeholder
    }
}

private struct FakeDocument: PdfDocument {
    let pages: [PageDimensions]
}

/// Requires `"right-password"` and otherwise mirrors the FFI's two-stage
/// failure: no password yet vs. a password that was tried and did not match.
private struct PasswordGatedClient: PdfCoreClient {
    let pages: [PageDimensions]

    func open(bytes: Data, password: String?) throws -> any PdfDocument {
        guard let password else { throw ViewerFailure.passwordRequired }
        guard password == "right-password" else { throw ViewerFailure.wrongPassword }
        return FakeDocument(pages: pages)
    }

    func render(document: any PdfDocument, page: Int, dpi: Int) throws -> RenderedPage {
        RenderedPage.placeholder
    }
}
