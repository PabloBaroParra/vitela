import CoreGraphics
import XCTest
@testable import Vitela

final class ViewerStorePrintTests: XCTestCase {
    private let letter = PageDimensions(width: 612, height: 792)
    private let a4Landscape = PageDimensions(width: 842, height: 595)

    func testBeginPrintWithoutADocumentSaysWhyAndPrintsNothing() {
        let store = ViewerStore(client: FakePdfCoreClient(pages: [letter]))

        XCTAssertNil(store.beginPrint())
        XCTAssertEqual(store.editStatus, "Open a PDF before printing.")
    }

    func testBeginPrintRefusesADocumentWithNoPages() {
        let store = ViewerStore(client: FakePdfCoreClient(pages: []))
        store.open(bytes: Data([1]))

        XCTAssertNil(store.beginPrint())
        XCTAssertEqual(store.editStatus, "The PDF has no pages to print.")
    }

    func testBeginPrintCarriesEveryPageInOrder() throws {
        let store = ViewerStore(client: FakePdfCoreClient(pages: [letter, a4Landscape]))
        store.open(bytes: Data([1]))

        let request = try XCTUnwrap(store.beginPrint())

        XCTAssertEqual(request.pages, [letter, a4Landscape])
        XCTAssertEqual(request.generation, store.generation)
    }

    func testPrintPageRendersAtTheRequestedDpiWithoutTouchingTheViewerSlots() throws {
        let client = FakePdfCoreClient(pages: [letter, a4Landscape])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let request = try XCTUnwrap(store.beginPrint())

        XCTAssertNotNil(store.printPage(1, for: request, dpi: ViewerStore.printDpi))

        XCTAssertEqual(client.requestedPages, [1])
        XCTAssertEqual(client.requestedDpi, [300])
        XCTAssertEqual(store.pageSlots.map(\.status), [.idle, .idle], "printing must not mark viewer pages as rendering")
    }

    /// A document opened while the print panel is up must not change what
    /// the job already asked for.
    func testARequestKeepsPrintingTheDocumentItWasTakenFrom() throws {
        let client = FakePdfCoreClient(pages: [letter])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let request = try XCTUnwrap(store.beginPrint())

        store.open(bytes: Data([2]))

        XCTAssertNotEqual(request.generation, store.generation)
        XCTAssertEqual(request.pages, [letter])
        XCTAssertNotNil(store.printPage(0, for: request, dpi: ViewerStore.printDpi))
    }

    func testAPageThatFailsToRenderPrintsBlankInsteadOfFailingTheJob() throws {
        let client = FakePdfCoreClient(pages: [letter])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let request = try XCTUnwrap(store.beginPrint())
        client.renderError = ViewerFailure.renderFailed(page: 0, message: "raster too large")

        XCTAssertNil(store.printPage(0, for: request, dpi: ViewerStore.printDpi))
    }

    func testAShortBufferIsNeverHandedToCoreGraphics() throws {
        let client = FakePdfCoreClient(pages: [letter])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let request = try XCTUnwrap(store.beginPrint())
        client.renderedImage = RenderedPage(rgba: Data([0, 0, 0, 0]), width: 2, height: 2, stride: 8)

        XCTAssertNil(store.printPage(0, for: request, dpi: ViewerStore.printDpi))
    }

    func testAPageOutsideTheRequestIsNotRendered() throws {
        let client = FakePdfCoreClient(pages: [letter])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let request = try XCTUnwrap(store.beginPrint())

        XCTAssertNil(store.printPage(1, for: request, dpi: ViewerStore.printDpi))
        XCTAssertTrue(client.requestedPages.isEmpty)
    }

    /// The open document renders its preview, which leaves this session's
    /// annotations out for the overlay to paint — and paper has no overlay.
    func testPrintDrawsTheOutputSnapshotNotTheOpenDocument() throws {
        let client = FakePdfCoreClient(pages: [letter, a4Landscape])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        let request = try XCTUnwrap(store.beginPrint())
        XCTAssertNotNil(store.printPage(0, for: request, dpi: ViewerStore.printDpi))
        XCTAssertNotNil(store.printPage(1, for: request, dpi: ViewerStore.printDpi))

        XCTAssertEqual(client.snapshotsTaken, 1, "one snapshot serves the whole job")
        XCTAssertEqual(client.renderedSnapshotPages, [0, 1])
    }

    /// Never a fallback to the open document: that is a printout without the edits.
    func testASnapshotThatFailsPrintsNothingAndSaysWhy() {
        let client = FakePdfCoreClient(pages: [letter])
        client.snapshotError = EditFailure.failed("The document could not be processed.")
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        XCTAssertNil(store.beginPrint())
        XCTAssertEqual(store.editStatus, "Could not print: The document could not be processed.")
        XCTAssertTrue(client.requestedPages.isEmpty)
    }

    func testFitScalesAPortraitPageToTheHeightAndCentresIt() {
        let rect = PrintLayout.fit(letter, in: CGRect(x: 10, y: 20, width: 1000, height: 792))

        XCTAssertEqual(rect.height, 792, accuracy: 0.001)
        XCTAssertEqual(rect.width, 612, accuracy: 0.001)
        XCTAssertEqual(rect.midX, 510, accuracy: 0.001)
        XCTAssertEqual(rect.minY, 20, accuracy: 0.001)
    }

    func testFitScalesALandscapePageOntoPortraitPaperByItsWidth() {
        let rect = PrintLayout.fit(a4Landscape, in: CGRect(x: 0, y: 0, width: 421, height: 1000))

        XCTAssertEqual(rect.width, 421, accuracy: 0.001)
        XCTAssertEqual(rect.height, 297.5, accuracy: 0.001)
        XCTAssertEqual(rect.midY, 500, accuracy: 0.001)
    }

    func testFitOfADegeneratePageIsEmpty() {
        XCTAssertEqual(PrintLayout.fit(PageDimensions(width: 0, height: 792), in: CGRect(x: 0, y: 0, width: 100, height: 100)), .zero)
    }
}
