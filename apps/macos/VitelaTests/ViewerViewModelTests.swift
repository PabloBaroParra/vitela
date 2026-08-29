import XCTest
@testable import Vitela

final class ViewerViewModelTests: XCTestCase {
    /// `ViewerRootView` observes only `ViewerViewModel` (`@ObservedObject var
    /// model`), never `store` directly — so if a `store`-only mutation (like a
    /// page finishing its render) doesn't also notify `model`, SwiftUI never
    /// re-renders and every `PageView` stays frozen on its initial
    /// "Rendering page N…" placeholder forever, no matter how long the wait.
    /// This reproduces that mutation — a render completing — while `title`
    /// (a `@Published` property already on `model`) stays untouched, so the
    /// only way `objectWillChange` can fire here is the forwarding subscription.
    func testViewModelNotifiesObserversWhenAPageFinishesRenderingEvenThoughTitleIsUnchanged() throws {
        let client = FakePagesClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))
        let titleBeforeRender = model.title

        var notified = false
        let subscription = model.objectWillChange.sink { notified = true }
        defer { subscription.cancel() }

        model.render(page: 0)

        let rendered = expectation(description: "page render reaches the store")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { rendered.fulfill() }
        wait(for: [rendered], timeout: 2)

        XCTAssertEqual(model.store.pageSlots[0].status, .rendered)
        XCTAssertEqual(model.title, titleBeforeRender)
        XCTAssertTrue(notified, "a render finishing only changes store.pageSlots; ViewerViewModel must forward store.objectWillChange or views bound to `model` alone never learn about it")
    }

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

    func testOpenSampleLoadsBytesFromTheInjectedLoaderAndOpensThem() throws {
        let client = FakePagesClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client), sampleLoader: { Data([9, 9, 9]) })

        model.openSample()

        // `openSample` runs on its own queue, same as `open(url:)`, so the
        // assertion has to wait for the result to land back on main.
        let loaded = expectation(description: "sample document reaches the store")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { loaded.fulfill() }
        wait(for: [loaded], timeout: 2)

        XCTAssertEqual(model.store.state, .loaded)
    }

    func testOpenSampleWithTheDefaultLoaderReadsTheBundledResource() throws {
        // No `sampleLoader` override: this exercises the real
        // `Bundle.main.url(forResource:withExtension:)` lookup against the
        // "vitela-sample.pdf" Resources build phase, not a test double.
        let client = FakePagesClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))

        model.openSample()

        let loaded = expectation(description: "bundled sample document reaches the store")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { loaded.fulfill() }
        wait(for: [loaded], timeout: 2)

        XCTAssertEqual(model.store.state, .loaded)
    }

    func testOpenSampleLoaderFailureIsReportedAsAReadFailure() throws {
        let model = ViewerViewModel(store: ViewerStore(client: EmptyClient()), sampleLoader: { throw SampleUnavailable() })

        model.openSample()

        let errored = expectation(description: "sample load failure reaches the store")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { errored.fulfill() }
        wait(for: [errored], timeout: 2)

        XCTAssertEqual(model.store.state, .error(.readFailed("sample document is missing")))
    }
}

private struct SampleUnavailable: Error, LocalizedError {
    var errorDescription: String? { "sample document is missing" }
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
