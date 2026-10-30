import XCTest
@testable import Vitela

/// Edit Content's text blocks (B25 T-207): runs the core reads as one piece
/// of text open, move, retype and delete as one — Windows' `TextBlockSmoke`.
/// A block of one keeps the single-run edits it always had.
final class ViewerStoreTextBlockTests: XCTestCase {
    private let left = ContentEditFixture.run(1, PdfRect(x: 100, y: 700, width: 40, height: 12), text: "PRESUPUES")
    private let right = ContentEditFixture.run(2, PdfRect(x: 140, y: 700, width: 30, height: 12), text: "TO Nº 3")
    private let alone = ContentEditFixture.run(3, PdfRect(x: 100, y: 600, width: 30, height: 12), text: "Pag 1")
    private var block: ContentTextBlock {
        ContentTextBlock(runs: [left, right], bbox: PdfRect(x: 100, y: 700, width: 70, height: 12), text: "PRESUPUESTO Nº 3")
    }

    private func opened() -> (FakePdfCoreClient, ViewerStore) {
        let (client, store) = ContentEditFixture.opened()
        client.pageContents[0] = PageContent(textRuns: [left, right, alone], images: [],
                                             textBlocks: [block, ContentTextBlock(alone)])
        store.setContentEditMode(true)
        ContentEditFixture.read(store)
        return (client, store)
    }

    private func perform(_ store: ViewerStore, _ edit: ContentEdit) throws {
        let request = try XCTUnwrap(store.beginContentEdit(edit))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
        ContentEditFixture.read(store)
    }

    func testAClickOnEitherRunOpensTheWholeBlock() {
        let (client, store) = opened()
        for x in [110.0, 160.0] {
            XCTAssertTrue(store.beginContentPress(page: 0, at: PdfPoint(x: x, y: 705), zoom: 1))
            guard case let .open(opened) = store.endContentDrag(at: PdfPoint(x: x, y: 705)) else { return XCTFail("a click opens") }
            XCTAssertEqual(opened, block)
            XCTAssertEqual(opened.text, "PRESUPUESTO Nº 3")
        }
        XCTAssertTrue(store.beginContentPress(page: 0, at: PdfPoint(x: 110, y: 605), zoom: 1))
        guard case let .open(single) = store.endContentDrag(at: PdfPoint(x: 110, y: 605)) else { return XCTFail("a click opens") }
        XCTAssertEqual(single, ContentTextBlock(alone))
        XCTAssertFalse(single.isMultiRun)
        XCTAssertTrue(client.contentEdits.isEmpty)
    }

    func testADragMovesTheWholeBlockAsOneStepAndThenHoldsItsRetype() {
        let (client, store) = opened()
        XCTAssertTrue(store.beginContentPress(page: 0, at: PdfPoint(x: 160, y: 705), zoom: 1))
        store.continueContentDrag(to: PdfPoint(x: 180, y: 685))
        XCTAssertEqual(store.contentDrag?.previewRect, PdfRect(x: 120, y: 680, width: 70, height: 12), "the block's box is dragged")
        guard case let .move(request) = store.endContentDrag(at: PdfPoint(x: 180, y: 685)) else { return XCTFail("a drag moves") }
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertEqual(client.contentEdits, [.moveTextBlock(block, dx: 20, dy: -20)])
        XCTAssertEqual(store.editStatus, "Text moved. Changes are pending save.")
        XCTAssertTrue(store.canUndo)

        ContentEditFixture.read(store)
        XCTAssertTrue(store.beginContentPress(page: 0, at: PdfPoint(x: 130, y: 685), zoom: 1))
        if case .open = store.endContentDrag(at: PdfPoint(x: 130, y: 685)) { XCTFail("a moved block is held until saved") }
        XCTAssertEqual(store.editStatus, ContentEditing.movedNotSaved)

        // Where the core checkpoints (#506), the moved block opens to retype.
        client.canCheckpoint = true
        XCTAssertTrue(store.beginContentPress(page: 0, at: PdfPoint(x: 130, y: 685), zoom: 1))
        guard case let .open(opened) = store.endContentDrag(at: PdfPoint(x: 130, y: 685)) else { return XCTFail("opens once the core can checkpoint") }
        XCTAssertEqual(opened.runs.map(\.id), [left.id, right.id])
    }

    func testTheEditorsRetypeDeleteAndTypedMoveUseTheBlockCommands() throws {
        let (client, store) = opened()
        XCTAssertNil(store.beginContentEdit(ViewerStore.textRetype(block, to: block.text)), "unchanged text records nothing")
        XCTAssertNil(store.beginContentEdit(ViewerStore.textRetype(block, to: "  ")))
        XCTAssertEqual(store.editStatus, ContentEditing.emptyRetype)

        try perform(store, ViewerStore.textRetype(block, to: "PRESUPUESTO Nº 4"))
        XCTAssertEqual(client.contentEdits, [.retypeTextBlock(block, to: "PRESUPUESTO Nº 4")])
        XCTAssertEqual(store.pageContent[0]?.textRuns.map(\.text), ["PRESUPUESTO Nº 4", "Pag 1"], "all of it in the first run")

        let retyped = try XCTUnwrap(store.pageContent[0]?.textBlocks.first)
        let (problem, request) = store.beginTextMove(retyped, toX: 72, y: 650)
        XCTAssertNil(problem)
        store.applyPageWork(store.pageWorkResult(for: try XCTUnwrap(request)), for: try XCTUnwrap(request))
        XCTAssertEqual(client.contentEdits.last, .moveText(retyped.first, to: PdfRect(x: 72, y: 650, width: 40, height: 12)),
                       "a block of one keeps the single-run move")

        let (otherClient, other) = opened()
        try perform(other, ViewerStore.textRemoval(block))
        XCTAssertEqual(otherClient.contentEdits, [.removeTextBlock(block)])
        XCTAssertEqual(other.pageContent[0]?.textRuns, [alone])
        XCTAssertEqual(ViewerStore.textRemoval(ContentTextBlock(alone)), .removeText(alone))
        XCTAssertEqual(ViewerStore.textMove(block, to: PdfRect(x: 90, y: 710, width: 70, height: 12)),
                       .moveTextBlock(block, dx: -10, dy: 10))
    }

    func testAPageWithoutBlocksTreatsEveryRunAsItsOwn() {
        let content = PageContent(textRuns: [left, right], images: [])
        XCTAssertEqual(content.block(containing: right), ContentTextBlock(right))
    }
}
