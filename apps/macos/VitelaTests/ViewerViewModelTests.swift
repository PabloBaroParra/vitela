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

        waitUntil(
            { model.store.pageSlots.first?.status == .rendered },
            "the render never reached the store"
        )

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
        waitUntil(
            { model.store.state == .loaded },
            "the sample document never reached the store"
        )

        XCTAssertEqual(model.store.state, .loaded)
    }

    func testOpenSampleWithTheDefaultLoaderReadsTheBundledResource() throws {
        // No `sampleLoader` override: this exercises the real
        // `Bundle.main.url(forResource:withExtension:)` lookup against the
        // "vitela-sample.pdf" Resources build phase, not a test double.
        let client = FakePagesClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))

        model.openSample()

        waitUntil({ model.store.state == .loaded }, "the bundled sample never reached the store")

        XCTAssertEqual(model.store.state, .loaded)
    }

    func testOpenAes128SampleReadsTheBundledResource() throws {
        // Exercises the real `Bundle.main.url(forResource:withExtension:)`
        // lookup against the "aes_128_user_and_owner.pdf" Resources build
        // phase entry, not a test double.
        let client = FakePagesClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))

        model.openAes128Sample()

        waitUntil(
            { model.store.state == .loaded },
            "the bundled AES-128 sample never reached the store"
        )

        XCTAssertEqual(model.store.state, .loaded)
    }

    func testOpenRc4128SampleReadsTheBundledResource() throws {
        let client = FakePagesClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))

        model.openRc4128Sample()

        waitUntil(
            { model.store.state == .loaded },
            "the bundled RC4-128 sample never reached the store"
        )

        XCTAssertEqual(model.store.state, .loaded)
    }

    func testOpenSampleLoaderFailureIsReportedAsAReadFailure() throws {
        let model = ViewerViewModel(store: ViewerStore(client: EmptyClient()), sampleLoader: { throw SampleUnavailable() })

        model.openSample()

        waitUntil(
            { model.store.state == .error(.readFailed("sample document is missing")) },
            "the sample load failure never reached the store"
        )

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

/// Turns the main run loop until `condition` holds, and fails if it never
/// does.
///
/// Every asynchronous path exercised in this file lands its result with
/// `DispatchQueue.main.async`, so the main run loop is the thing that has to
/// turn before an assertion has anything new to look at. Spinning it in short
/// slices returns as soon as the result is in — a millisecond or two, rather
/// than a fixed sleep.
///
/// It replaces this, which every asynchronous test here used to spell out:
///
/// ```swift
/// let done = expectation(description: "…")
/// DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { done.fulfill() }
/// wait(for: [done], timeout: 2)
/// ```
///
/// That is a `sleep` wearing an `XCTestExpectation`'s clothes. The timer
/// fulfilled the expectation on schedule whether or not the work had
/// finished, so `timeout:` never applied to anything and the assertion simply
/// read whatever state happened to be there at 0.5 s. On a loaded CI runner
/// that is the *previous* state:
/// `testUnreadableUrlIsReportedAsAReadFailureAndKeepsTheOpenDocument` failed
/// with "expected a read failure, got loaded", `loaded` being the state set
/// two lines above it by `store.open(bytes:)`.
///
/// The timeout here is generous because it is now a real deadline rather than
/// a sleep: a passing test never waits for it, so making it long costs
/// nothing and makes a slow machine a slow run instead of a red one.
private extension XCTestCase {
    func waitUntil(
        _ condition: () -> Bool,
        _ message: @autoclosure () -> String,
        timeout: TimeInterval = 5,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        let deadline = Date().addingTimeInterval(timeout)
        while !condition() && Date() < deadline {
            _ = RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.01))
        }
        XCTAssertTrue(condition(), message(), file: file, line: line)
    }
}
