import XCTest
@testable import Vitela

/// Edit Content's clipboard (B25 T-209): copy and cut what is selected,
/// paste it where asked, each edit one undo step — Windows'
/// `ContentClipboardSmoke`.
final class ViewerStoreContentClipboardTests: XCTestCase {
    private let frame = ContentPath(id: 7, page: 0, bbox: PdfRect(x: 300, y: 600, width: 200, height: 100), paint: .stroke)
    private let cell = ContentEditFixture.run(1, PdfRect(x: 320, y: 640, width: 60, height: 12), text: "Total")

    private func opened() -> (FakePdfCoreClient, ViewerStore) {
        let (client, store) = ContentEditFixture.opened(FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792),
                                                                                 PageDimensions(width: 612, height: 792)]))
        client.pageContents[0] = PageContent(textRuns: [cell], images: [], paths: [frame], textBlocks: [ContentTextBlock(cell)])
        store.setContentEditMode(true)
        ContentEditFixture.read(store)
        return (client, store)
    }

    private func marquee(_ store: ViewerStore) {
        XCTAssertTrue(store.beginContentPress(page: 0, at: PdfPoint(x: 250, y: 750), zoom: 1))
        store.continueContentDrag(to: PdfPoint(x: 330, y: 645))
        _ = store.endContentDrag(at: PdfPoint(x: 330, y: 645))
    }

    func testCopyHoldsTheSelectionAndPasteIsOneStepOnAnyPage() throws {
        let (client, store) = opened()
        XCTAssertNil(store.copyContentSelection())
        XCTAssertEqual(store.editStatus, ContentEditing.nothingToCopy)

        marquee(store)
        let selection = try XCTUnwrap(store.contentSelection)
        let (clip, copied) = try XCTUnwrap(store.copyContentSelection())
        XCTAssertEqual(copied, selection)
        XCTAssertEqual(client.copiedSelections, [selection])
        XCTAssertEqual(clip.bbox, selection.bbox)
        XCTAssertEqual(store.editStatus, ContentEditing.copied(2, cut: false))
        XCTAssertTrue(client.contentEdits.isEmpty, "a copy changes nothing")

        let request = try XCTUnwrap(store.beginContentPaste(clip, page: 1, at: PdfPoint(x: 72, y: 720)))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
        XCTAssertEqual(client.contentEdits, [.pasteContent(page: 1, clip: clip, left: 72, top: 720)])
        XCTAssertEqual(store.editStatus, "Pasted. Changes are pending save.")
        XCTAssertTrue(store.canUndo)
        XCTAssertNil(store.beginContentPaste(clip, page: 5, at: PdfPoint(x: 0, y: 0)), "no such page")
    }

    func testASingleSelectedShapeIsCopiedAndCutRemovesItAsOneStep() throws {
        let (client, store) = opened()
        XCTAssertTrue(store.beginContentPress(page: 0, at: PdfPoint(x: 300, y: 680), zoom: 1))
        _ = store.endContentDrag(at: PdfPoint(x: 300, y: 680))
        XCTAssertEqual(store.selectedContentPath, frame)
        XCTAssertEqual(store.clipboardSelection?.paths, [frame])

        let (_, request) = try XCTUnwrap(store.beginContentCut())
        XCTAssertNil(store.selectedContentPath)
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
        XCTAssertEqual(client.contentEdits.count, 1)
        guard case let .removeSelection(removed) = client.contentEdits.first else { return XCTFail("cut removes") }
        XCTAssertEqual(removed.paths, [frame])
    }

    func testARefusedCopySaysWhyAndCutsNothing() {
        let (client, store) = opened()
        marquee(store)
        client.contentEditError = EditFailure.notPermitted(ContentEditing.copyRefused)
        XCTAssertNil(store.beginContentCut())
        XCTAssertEqual(store.editStatus, ContentEditing.copyRefused)
        XCTAssertTrue(client.contentEdits.isEmpty)
        XCTAssertNotNil(store.contentSelection, "the selection stays for another try")
        store.setContentEditMode(false)
        XCTAssertNil(store.clipboardSelection)
    }
}
