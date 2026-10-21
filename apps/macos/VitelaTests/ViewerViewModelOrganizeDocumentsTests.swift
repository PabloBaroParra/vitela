import XCTest
@testable import Vitela

/// Organize's Documents view on macOS: it is where Organize opens, and a
/// block is dragged by a one-time token that only its own drag carries.
final class ViewerViewModelOrganizeDocumentsTests: XCTestCase {
    private let page = PageDimensions(width: 612, height: 792)

    private func model() -> (FakePdfCoreClient, ViewerViewModel) {
        let client = FakePdfCoreClient(pages: Array(repeating: page, count: 3))
        client.blocks = [
            DocumentBlock(source: .base, part: nil, start: 0, count: 2),
            DocumentBlock(source: .blank, part: nil, start: 2, count: 1),
        ]
        let model = ViewerViewModel(store: ViewerStore(client: client), sampleLoader: { Data([1]) })
        model.openSample()
        waitUntil({ model.store.state == .loaded }, "the sample never opened")
        return (client, model)
    }

    func testOrganizeOpensOnDocumentsAndSwitchingSaysWhatToDrag() {
        let (_, model) = model()
        XCTAssertTrue(model.organizeShowsDocuments)

        model.showOrganizeDocuments(false)
        XCTAssertEqual(model.store.editStatus, DocumentBlocks.pagesHint)
        model.showOrganizeDocuments(true)
        XCTAssertEqual(model.store.editStatus, DocumentBlocks.documentsHint)
    }

    func testADroppedBlockMovesOnlyWithItsOwnToken() {
        let (client, model) = model()

        XCTAssertFalse(model.dropBlock(token: "pasted text", atSlot: 0))
        let token = model.beginBlockDrag(position: 1)
        XCTAssertFalse(model.dropBlock(token: "pasted text", atSlot: 0))
        XCTAssertTrue(model.dropBlock(token: token, atSlot: 0))

        waitUntil({ client.pageEdits == [.moveBlock(from: 2, count: 1, to: 0)] }, "the block never moved")
        XCTAssertFalse(model.dropBlock(token: token, atSlot: 2), "a token is spent by its drop")
    }

    func testTheOpenFilesBlockIsCalledByItsName() {
        let (_, model) = model()
        model.documentName = "report.pdf"
        XCTAssertEqual(model.store.documentBlockRows(baseName: model.baseBlockName).map(\.title), ["report.pdf", "Blank pages"])
    }
}
