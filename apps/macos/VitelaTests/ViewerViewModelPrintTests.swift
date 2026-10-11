import AppKit
import XCTest
@testable import Vitela

final class ViewerViewModelPrintTests: XCTestCase {
    private let letter = PageDimensions(width: 612, height: 792)
    private let a4Landscape = PageDimensions(width: 842, height: 595)

    /// US Letter with half-inch margins: a 540 × 720 printable area.
    private func letterPrintInfo() -> NSPrintInfo {
        let info = NSPrintInfo()
        info.paperSize = NSSize(width: 612, height: 792)
        info.leftMargin = 36
        info.rightMargin = 36
        info.topMargin = 36
        info.bottomMargin = 36
        return info
    }

    func testThereIsNoPrintOperationWithoutADocument() {
        let model = ViewerViewModel(store: ViewerStore(client: FakePdfCoreClient(pages: [letter])))

        XCTAssertNil(model.makePrintOperation())
        XCTAssertEqual(model.store.editStatus, "Open a PDF before printing.")
    }

    func testThePrintOperationShowsThePanelAndPaginatesEveryPage() throws {
        let model = ViewerViewModel(store: ViewerStore(client: FakePdfCoreClient(pages: [letter, a4Landscape, letter])))
        model.store.open(bytes: Data([1]))

        let operation = try XCTUnwrap(model.makePrintOperation())

        XCTAssertTrue(operation.showsPrintPanel)
        XCTAssertTrue(operation.showsProgressPanel)
        XCTAssertTrue(operation.view is PrintPagesView)
        var range = NSRange()
        XCTAssertTrue(operation.view?.knowsPageRange(&range) ?? false)
        XCTAssertEqual(range, NSRange(location: 1, length: 3))
    }

    func testEachPrinterPageIsOnePrintableAreaStackedTopToBottom() {
        let view = PrintPagesView(pages: [letter, letter], printInfo: letterPrintInfo()) { _, _ in nil }

        XCTAssertEqual(view.rectForPage(1), NSRect(x: 0, y: 0, width: 540, height: 720))
        XCTAssertEqual(view.rectForPage(2), NSRect(x: 0, y: 720, width: 540, height: 720))
        XCTAssertEqual(view.frame.size, NSSize(width: 540, height: 1440))
    }

    /// Runs the same `NSPrintOperation(view:printInfo:)` path a real job
    /// takes — `knowsPageRange`, `rectForPage`, one `draw` per page — but
    /// with the job saved to a PDF file, so it needs neither a panel nor a
    /// printer. (`NSPrintOperation.pdfOperation(with:inside:…)` would not do:
    /// it draws the whole rect as one page and skips the pagination.)
    private func printToFile(_ view: NSView, _ info: NSPrintInfo) throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("vitela-print-\(UUID().uuidString).pdf")
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        info.jobDisposition = .save
        info.dictionary()[NSPrintInfo.AttributeKey.jobSavingURL] = url
        let operation = NSPrintOperation(view: view, printInfo: info)
        operation.showsPrintPanel = false
        operation.showsProgressPanel = false
        XCTAssertTrue(operation.run())
        return url
    }

    func testPrintingRendersEachPageOnceAtPrintDpiAndEmitsOnePaperPagePerPage() throws {
        let client = FakePdfCoreClient(pages: [letter, a4Landscape])
        client.renderedImage = RenderedPage(rgba: Data(repeating: 255, count: 4 * 4), width: 2, height: 2, stride: 8)
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let request = try XCTUnwrap(store.beginPrint())
        let info = letterPrintInfo()
        let view = PrintPagesView(pages: request.pages, printInfo: info) { page, dpi in
            store.printPage(page, for: request, dpi: dpi)
        }

        let printed = try printToFile(view, info)

        XCTAssertEqual(client.requestedPages.sorted(), [0, 1])
        XCTAssertEqual(Set(client.requestedDpi), [ViewerStore.printDpi])
        XCTAssertEqual(CGPDFDocument(printed as CFURL)?.numberOfPages, 2)
    }

    func testAPageThatFailsToRenderStillPrintsAsABlankPage() throws {
        let client = FakePdfCoreClient(pages: [letter, letter])
        client.renderError = ViewerFailure.renderFailed(page: 0, message: "boom")
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let request = try XCTUnwrap(store.beginPrint())
        let info = letterPrintInfo()
        let view = PrintPagesView(pages: request.pages, printInfo: info) { page, dpi in
            store.printPage(page, for: request, dpi: dpi)
        }

        let printed = try printToFile(view, info)

        XCTAssertEqual(CGPDFDocument(printed as CFURL)?.numberOfPages, 2)
    }
}
