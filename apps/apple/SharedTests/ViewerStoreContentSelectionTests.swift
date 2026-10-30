import XCTest
@testable import Vitela

/// Edit Content's multi-selection (B25 T-208): a marquee from empty page,
/// Shift-click, a drag of everything selected and Delete, each one undo
/// step — Windows' `ContentSelectionSmoke`.
final class ViewerStoreContentSelectionTests: XCTestCase {
    private let frame = ContentPath(id: 7, page: 0, bbox: PdfRect(x: 300, y: 600, width: 200, height: 100), paint: .stroke)
    private let cell = ContentEditFixture.run(1, PdfRect(x: 320, y: 640, width: 60, height: 12), text: "Total")
    private let far = ContentEditFixture.run(2, PdfRect(x: 72, y: 100, width: 60, height: 12), text: "Pag 1")
    private let logo = ContentImage(id: 40, page: 0, bbox: PdfRect(x: 72, y: 600, width: 100, height: 50), resourceXObjectName: "Im1")

    private func opened() -> (FakePdfCoreClient, ViewerStore) {
        let (client, store) = ContentEditFixture.opened()
        client.pageContents[0] = PageContent(textRuns: [cell, far], images: [logo], paths: [frame],
                                             textBlocks: [ContentTextBlock(cell), ContentTextBlock(far)])
        store.setContentEditMode(true)
        ContentEditFixture.read(store)
        return (client, store)
    }

    @discardableResult
    private func press(_ store: ViewerStore, _ from: PdfPoint, to: PdfPoint? = nil, extend: Bool = false) -> ContentRelease? {
        guard store.beginContentPress(page: 0, at: from, zoom: 1, extend: extend) else { return nil }
        if let to { store.continueContentDrag(to: to) }
        let release = store.endContentDrag(at: to ?? from)
        if case let .move(request) = release {
            store.applyPageWork(store.pageWorkResult(for: request), for: request)
            ContentEditFixture.read(store)
        }
        return release
    }

    func testAMarqueeFromEmptyPageGathersWhatItTouches() throws {
        let (client, store) = opened()
        // From empty page, across the frame's corner: the frame and its cell.
        press(store, PdfPoint(x: 250, y: 750), to: PdfPoint(x: 330, y: 645))
        let selection = try XCTUnwrap(store.contentSelection)
        XCTAssertEqual(selection.paths, [frame])
        XCTAssertEqual(selection.blocks, [ContentTextBlock(cell)])
        XCTAssertTrue(selection.images.isEmpty)
        XCTAssertEqual(store.editStatus, ContentEditing.selected(2))
        XCTAssertTrue(client.contentEdits.isEmpty)

        press(store, PdfPoint(x: 250, y: 300), to: PdfPoint(x: 260, y: 310))
        XCTAssertNil(store.contentSelection)
        XCTAssertEqual(store.editStatus, ContentEditing.nothingToSelect)

        press(store, PdfPoint(x: 250, y: 300))
        XCTAssertEqual(store.editStatus, ContentEditing.missed, "a plain click on empty page still says so")
    }

    func testShiftClickTogglesAndExtendsASingleSelection() throws {
        let (_, store) = opened()
        press(store, PdfPoint(x: 100, y: 620))
        XCTAssertEqual(store.selectedContentImage, logo, "a plain click is still a single selection")
        press(store, PdfPoint(x: 330, y: 645), extend: true)
        var selection = try XCTUnwrap(store.contentSelection)
        XCTAssertEqual(selection.images, [logo], "Shift extends the image already selected")
        XCTAssertEqual(selection.blocks, [ContentTextBlock(cell)])
        XCTAssertNil(store.selectedContentImage, "only one of the two selections is ever set")

        press(store, PdfPoint(x: 330, y: 645), extend: true)
        selection = try XCTUnwrap(store.contentSelection)
        XCTAssertEqual(selection.count, 1, "a second Shift-click removes it")

        press(store, PdfPoint(x: 200, y: 130), to: PdfPoint(x: 100, y: 95), extend: true)
        selection = try XCTUnwrap(store.contentSelection)
        XCTAssertEqual(selection.count, 2, "a Shift marquee adds to what is selected")
    }

    func testADragInsideTheSelectionMovesItAllAsOneStepAndKeepsItSelected() throws {
        let (client, store) = opened()
        press(store, PdfPoint(x: 250, y: 750), to: PdfPoint(x: 330, y: 645))
        let selection = try XCTUnwrap(store.contentSelection)

        press(store, PdfPoint(x: 400, y: 650), to: PdfPoint(x: 420, y: 630))

        XCTAssertEqual(client.contentEdits, [.moveSelection(selection, dx: 20, dy: -20)])
        XCTAssertEqual(store.editStatus, "Selection moved. Changes are pending save.")
        XCTAssertTrue(store.canUndo)
        let kept = try XCTUnwrap(store.contentSelection, "the members stay selected where they landed")
        XCTAssertEqual(kept.paths.first?.bbox, PdfRect(x: 320, y: 580, width: 200, height: 100))
        XCTAssertEqual(kept.blocks.first?.bbox, PdfRect(x: 340, y: 620, width: 60, height: 12))
    }

    func testDeleteRemovesEveryMemberAsOneStep() throws {
        let (client, store) = opened()
        XCTAssertNil(store.beginSelectionDelete())
        press(store, PdfPoint(x: 250, y: 750), to: PdfPoint(x: 330, y: 645))
        let selection = try XCTUnwrap(store.contentSelection)
        let request = try XCTUnwrap(store.beginSelectionDelete())
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
        ContentEditFixture.read(store)
        XCTAssertEqual(client.contentEdits, [.removeSelection(selection)])
        XCTAssertEqual(store.editStatus, "Selection deleted. Changes are pending save.")
        XCTAssertNil(store.contentSelection)
        XCTAssertEqual(store.pageContent[0]?.paths, [])
        XCTAssertEqual(store.pageContent[0]?.textRuns, [far])
    }

    func testAPressNearALineOrOnAnItemIsStillASingleItemGesture() {
        let (_, store) = opened()
        XCTAssertTrue(store.beginContentPress(page: 0, at: PdfPoint(x: 70, y: 105), zoom: 1))
        XCTAssertEqual(store.contentDrag?.run, far, "within reach of a line, not a marquee")
        XCTAssertNil(store.selectionGesture)
        store.cancelContentDrag()
        store.setContentEditMode(false)
        XCTAssertNil(store.contentSelection)
    }
}
