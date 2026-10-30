import AppKit
import XCTest
@testable import Vitela

/// ⌘C/⌘X/⌘V in Edit Content on macOS (B25 T-209): the clip is held by the
/// shell across documents, the text goes to the system clipboard, and a
/// paste lands where the pointer last was.
final class ViewerViewModelContentClipboardTests: XCTestCase {
    private let frame = ContentPath(id: 7, page: 0, bbox: PdfRect(x: 300, y: 600, width: 200, height: 100), paint: .stroke)
    private let cell = ContentTextRun(id: 1, page: 0, bbox: PdfRect(x: 320, y: 640, width: 60, height: 12),
                                      resourceFontName: "F1", fontKind: .standard14, text: "Total")

    func testCopyHoldsTheClipAndTextAndPasteLandsAtThePointer() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.pageContents[0] = PageContent(textRuns: [cell], images: [], paths: [frame], textBlocks: [ContentTextBlock(cell)])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))
        model.setContentEditMode(true)
        let read = try XCTUnwrap(model.store.beginPageContent(page: 0))
        model.store.applyPageContent(model.store.pageContentResult(for: read), for: read)
        XCTAssertTrue(model.store.beginContentPress(page: 0, at: PdfPoint(x: 250, y: 750), zoom: 1))
        model.store.continueContentDrag(to: PdfPoint(x: 330, y: 645))
        _ = model.store.endContentDrag(at: PdfPoint(x: 330, y: 645))

        let pasteboard = NSPasteboard(name: NSPasteboard.Name("vitela-test-\(UUID().uuidString)"))
        defer { pasteboard.releaseGlobally() }
        model.copyContent(pasteboard: pasteboard)
        let clip = try XCTUnwrap(model.contentClip)
        XCTAssertEqual(pasteboard.string(forType: .string), "Total", "other apps get the text")

        model.contentPointer = ContentPointer(page: 0, point: PdfPoint(x: 72, y: 500))
        model.pasteContent()
        waitUntil({ !model.store.pageWorkInFlight && !client.contentEdits.isEmpty }, "the paste never ran")
        XCTAssertEqual(client.contentEdits, [.pasteContent(page: 0, clip: clip, left: 72, top: 500)])

        // The clip outlives the document, for a paste into another one.
        model.store.open(bytes: Data([2]))
        XCTAssertEqual(model.contentClip, clip)
    }

    func testThePasteFallsBackToTheClipsOwnCornerWithNoPointer() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))
        model.setContentEditMode(true)
        let clip = ContentClip(pdf: Data([1]), bbox: PdfRect(x: 40, y: 50, width: 10, height: 20))
        model.contentClip = clip
        model.contentPointer = ContentPointer(page: 9, point: PdfPoint(x: 1, y: 1))
        model.pasteContent()
        waitUntil({ !client.contentEdits.isEmpty }, "the paste never ran")
        XCTAssertEqual(client.contentEdits, [.pasteContent(page: 0, clip: clip, left: 40, top: 70)])
    }
}
