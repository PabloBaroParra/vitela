import XCTest
@testable import Vitela

final class ViewerStoreTests: XCTestCase {
    func testOpenForwardsExactBytesAndCreatesOrderedPageSlots() throws {
        let client = FakePdfCoreClient(
            pages: [PageDimensions(width: 612, height: 792), PageDimensions(width: 595, height: 842)]
        )
        let store = ViewerStore(client: client)
        let bytes = Data([0x25, 0x50, 0x44, 0x46])

        store.open(bytes: bytes)

        XCTAssertEqual(client.openedBytes, [bytes])
        XCTAssertEqual(store.pageSlots.map(\.index), [0, 1])
        XCTAssertEqual(store.pageSlots.map(\.dimensions), client.pages)
        XCTAssertEqual(store.state, .loaded)
    }

    /// `pageSlots` reuses the same `index` values (0, 1, 2, …) across every
    /// document, so a view keying `ForEach` identity only on `slot.index`
    /// would treat a second document's rows as the *same* views as the
    /// first's — `onAppear` would never refire and pages would never render.
    /// `ViewerRootView` keys identity on `generation` too specifically to
    /// avoid that; this pins the contract it depends on.
    func testGenerationAdvancesOnEverySuccessfulOpenEvenWithTheSamePageCount() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)

        store.open(bytes: Data([1]))
        let firstGeneration = store.generation
        store.open(bytes: Data([2]))

        XCTAssertNotEqual(store.generation, firstGeneration)
        // Same single-page shape both times — this is exactly the case where
        // `slot.index` alone would collide across documents.
        XCTAssertEqual(store.pageSlots.map(\.index), [0])
    }

    func testOpenFailureIsRecoverableAndDoesNotReplaceExistingDocument() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        client.openError = .openFailed("invalid PDF")

        store.open(bytes: Data([2]))

        XCTAssertEqual(store.pageSlots.count, 1)
        XCTAssertEqual(store.state, .error(.openFailed("invalid PDF")))
    }

    func testUnreadableFileIsReportedAsReadFailureAndKeepsTheOpenDocument() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        store.reportOpenFailure(.readFailed("permission denied"))

        XCTAssertEqual(store.state, .error(.readFailed("permission denied")))
        XCTAssertEqual(store.pageSlots.count, 1)
        // The read never reached the core, so no second open was attempted.
        XCTAssertEqual(client.openedBytes.count, 1)
    }

    func testCancelSelectionLeavesCurrentStateUnchanged() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        store.selectionCancelled()

        XCTAssertEqual(store.state, .loaded)
        XCTAssertEqual(store.pageSlots.count, 1)
    }

    func testZoomClampsAtBothBoundsWithoutChangingPageOrder() throws {
        let client = FakePdfCoreClient(
            pages: [PageDimensions(width: 612, height: 792), PageDimensions(width: 595, height: 842)]
        )
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        store.setZoom(9)
        let upperOrder = store.pageSlots.map(\.index)
        XCTAssertEqual(store.zoom, ViewerStore.maximumZoom)
        store.setZoom(0.1)

        XCTAssertEqual(store.zoom, ViewerStore.minimumZoom)
        XCTAssertEqual(upperOrder, [0, 1])
        XCTAssertEqual(store.pageSlots.map(\.index), [0, 1])
    }

    func testStaleRenderCompletionDoesNotReplaceNewerGeneration() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let staleRequest = try XCTUnwrap(store.beginRender(page: 0))
        store.open(bytes: Data([2]))

        store.applyRender(.success(RenderedPage.placeholder), for: staleRequest)

        XCTAssertNil(store.pageSlots[0].image)
    }

    func testPageRenderFailureKeepsOtherPagesUsableAndCanRetry() throws {
        let client = FakePdfCoreClient(
            pages: [PageDimensions(width: 612, height: 792), PageDimensions(width: 595, height: 842)]
        )
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let failedRequest = try XCTUnwrap(store.beginRender(page: 1))

        store.applyRender(.failure(.renderFailed(page: 1, message: "damaged")), for: failedRequest)

        XCTAssertEqual(store.pageSlots[0].status, .idle)
        XCTAssertEqual(store.pageSlots[1].status, .failed("damaged"))
        // A failed page does not re-render on its own — otherwise every scroll
        // past it would restart the same doomed work.
        XCTAssertNil(store.beginRender(page: 1))
        XCTAssertNotNil(store.retry(page: 1))
    }

    func testRenderIsNotRequestedTwiceForAPageAlreadyCurrentAtThisZoom() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        let first = try XCTUnwrap(store.beginRender(page: 0))
        XCTAssertNil(store.beginRender(page: 0), "a render already in flight must not be duplicated")
        store.applyRender(store.renderResult(for: first), for: first)

        XCTAssertEqual(store.pageSlots[0].status, .rendered)
        XCTAssertNil(store.beginRender(page: 0), "a page current at this zoom must not re-render")
    }

    func testZoomChangeMakesRenderedPagesStaleAndRequestsTheScaledDpi() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let initial = try XCTUnwrap(store.beginRender(page: 0))
        store.applyRender(store.renderResult(for: initial), for: initial)
        XCTAssertEqual(initial.dpi, 72)

        store.setZoom(2)
        let rescaled = try XCTUnwrap(store.beginRender(page: 0), "zoom change must invalidate the render")

        XCTAssertEqual(rescaled.dpi, 144)
        // The previous bitmap stays on screen while the sharper one renders.
        XCTAssertNotNil(store.pageSlots[0].image)

        store.applyRender(store.renderResult(for: rescaled), for: rescaled)
        XCTAssertEqual(client.requestedDpi, [72, 144], "the core must be asked for the zoomed DPI, not an upscale")
        XCTAssertEqual(store.pageSlots[0].renderZoom, 2)
    }

    func testPasswordRequiredCanBeRetriedWithTheSameBytesAndSucceeds() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        client.openError = .passwordRequired

        store.open(bytes: Data([1]))
        XCTAssertEqual(store.state, .error(.passwordRequired))

        client.openError = nil
        store.retryPassword("correct-horse")

        XCTAssertEqual(store.state, .loaded)
        XCTAssertEqual(client.openedBytes, [Data([1]), Data([1])])
        XCTAssertEqual(client.openedPasswords, [nil, "correct-horse"])
    }

    func testWrongPasswordKeepsPromptingWithoutForgettingTheOriginalBytes() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        client.openError = .passwordRequired
        store.open(bytes: Data([9]))

        client.openError = .wrongPassword
        store.retryPassword("nope")

        XCTAssertEqual(store.state, .error(.wrongPassword))
        XCTAssertEqual(client.openedBytes, [Data([9]), Data([9])])
    }

    func testRetryPasswordWithoutAPriorOpenIsANoOp() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)

        store.retryPassword("anything")

        XCTAssertEqual(store.state, .empty)
        XCTAssertTrue(client.openedBytes.isEmpty)
    }

    func testRenderResultShorterThanItsStrideIsRejectedWithoutClosingTheDocument() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let request = try XCTUnwrap(store.beginRender(page: 0))
        // Declares 4 rows of 8 bytes but carries only one row: handing this to
        // Core Graphics is an out-of-bounds read.
        let truncated = RenderedPage(rgba: Data(count: 8), width: 2, height: 4, stride: 8)

        store.applyRender(.success(truncated), for: request)

        XCTAssertEqual(store.pageSlots[0].status, .failed(ViewerFailure.invalidImage(page: 0).message))
        XCTAssertNil(store.pageSlots[0].image)
        XCTAssertEqual(store.state, .loaded)
    }
}

private final class FakePdfCoreClient: PdfCoreClient {
    let pages: [PageDimensions]
    var openedBytes: [Data] = []
    var openedPasswords: [String?] = []
    var requestedDpi: [Int] = []
    var openError: ViewerFailure?

    init(pages: [PageDimensions]) {
        self.pages = pages
    }

    func open(bytes: Data, password: String?) throws -> any PdfDocument {
        openedBytes.append(bytes)
        openedPasswords.append(password)
        if let openError {
            throw openError
        }
        return FakeDocument(pages: pages)
    }

    func render(document: any PdfDocument, page: Int, dpi: Int) throws -> RenderedPage {
        requestedDpi.append(dpi)
        return RenderedPage.placeholder
    }
}

private struct FakeDocument: PdfDocument {
    let pages: [PageDimensions]
}
