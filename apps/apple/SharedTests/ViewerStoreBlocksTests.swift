import XCTest
@testable import Vitela

/// Organize's Documents view: what each block is called, and each card
/// action becoming one page edit over the whole block.
final class ViewerStoreBlocksTests: XCTestCase {
    private let page = PageDimensions(width: 612, height: 792)

    /// Base pages 0–1, blank page 2, an import over 3–5.
    private let blocks = [
        DocumentBlock(source: .base, part: nil, start: 0, count: 2),
        DocumentBlock(source: .blank, part: nil, start: 2, count: 1),
        DocumentBlock(source: .imported(id: 7), part: nil, start: 3, count: 3),
    ]

    private func opened(blocks: [DocumentBlock]? = nil, pages: Int = 6) -> (FakePdfCoreClient, ViewerStore) {
        let client = FakePdfCoreClient(pages: Array(repeating: page, count: pages))
        client.blocks = blocks ?? self.blocks
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return (client, store)
    }

    private func run(_ store: ViewerStore, _ request: ViewerStore.PageWorkRequest?) throws {
        let request = try XCTUnwrap(request)
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
    }

    func testBlocksAreReadOnOpenAndNamedByTheirSource() {
        let (_, store) = opened(blocks: [
            DocumentBlock(source: .base, part: 1, start: 0, count: 1),
            DocumentBlock(source: .blank, part: nil, start: 1, count: 2),
            DocumentBlock(source: .imported(id: 9), part: nil, start: 3, count: 2),
            DocumentBlock(source: .base, part: 2, start: 5, count: 1),
        ])

        let rows = store.documentBlockRows(baseName: "report.pdf")

        XCTAssertEqual(rows.map(\.title), ["report.pdf — Part 1", "Blank pages", "Imported PDF", "report.pdf — Part 2"])
        XCTAssertEqual(rows.map(\.meta), ["1 page · 1", "2 pages · 2–3", "2 pages · 4–5", "1 page · 6"])
    }

    func testAnImportedBlockIsCalledByThePickedFile() throws {
        let (client, store) = opened(blocks: [DocumentBlock(source: .base, part: nil, start: 0, count: 6)])
        client.blocks = nil
        let sources: [any ImportSource] = [FakeImportSource(pages: [page]), FakeImportSource(pages: [page, page])]

        try run(store, store.beginImport(sources, names: ["a.pdf", "b.pdf"]))
        client.blocks = [
            DocumentBlock(source: .base, part: nil, start: 0, count: 6),
            DocumentBlock(source: .imported(id: 41), part: nil, start: 6, count: 1),
            DocumentBlock(source: .imported(id: 42), part: nil, start: 7, count: 2),
        ]
        store.refreshDocumentBlocks()

        XCTAssertEqual(store.documentBlockRows(baseName: "base.pdf").map(\.name), ["base.pdf", "a.pdf", "b.pdf"])
    }

    func testRotatingAndDeletingTakeTheWholeBlock() throws {
        let (client, store) = opened()

        try run(store, store.beginBlockEdit(2, .rotateRight))
        XCTAssertEqual(store.editStatus, "Document rotated. Changes are pending save.")
        try run(store, store.beginBlockEdit(0, .rotateLeft))
        try run(store, store.beginBlockEdit(2, .delete))

        XCTAssertEqual(client.pageEdits, [
            .rotateBlock(from: 3, count: 3, degrees: 90),
            .rotateBlock(from: 0, count: 2, degrees: -90),
            .removeBlock(from: 3, count: 3),
        ])
        XCTAssertEqual(store.editStatus, "Document deleted. Changes are pending save.")
        XCTAssertEqual(store.pageSlots.count, 3)
    }

    func testMovingABlockToAGapStartsItWhereTheGapIs() throws {
        // Windows' `ResolveBlockMove`: `to` is where the block starts after.
        XCTAssertEqual(DocumentBlocks.move(0, toSlot: 3, in: blocks), .moveBlock(from: 0, count: 2, to: 4))
        XCTAssertEqual(DocumentBlocks.move(2, toSlot: 0, in: blocks), .moveBlock(from: 3, count: 3, to: 0))
        XCTAssertEqual(DocumentBlocks.move(1, toSlot: 3, in: blocks), .moveBlock(from: 2, count: 1, to: 5))
        XCTAssertNil(DocumentBlocks.move(1, toSlot: 1, in: blocks), "the gap before itself")
        XCTAssertNil(DocumentBlocks.move(1, toSlot: 2, in: blocks), "the gap after itself")
        XCTAssertNil(DocumentBlocks.move(1, toSlot: 4, in: blocks))
        XCTAssertNil(DocumentBlocks.move(3, toSlot: 0, in: blocks))

        let (client, store) = opened()
        try run(store, store.beginBlockEdit(0, .move(toSlot: 3)))
        XCTAssertEqual(client.pageEdits, [.moveBlock(from: 0, count: 2, to: 4)])
        XCTAssertEqual(store.editStatus, "Document moved. Changes are pending save.")
    }

    func testTheOnlyBlockCannotBeDeleted() {
        let (client, store) = opened(blocks: [DocumentBlock(source: .base, part: nil, start: 0, count: 6)])

        XCTAssertNil(store.beginBlockEdit(0, .delete))

        XCTAssertEqual(store.editStatus, "A document needs at least one page.")
        XCTAssertTrue(client.pageEdits.isEmpty)
    }

    func testANewDocumentForgetsTheLastOnesImportNames() throws {
        let (client, store) = opened(blocks: [DocumentBlock(source: .base, part: nil, start: 0, count: 6)])
        try run(store, store.beginImport([FakeImportSource(pages: [page])], names: ["a.pdf"]))
        XCTAssertEqual(store.importedSourceNames, [41: "a.pdf"])

        store.open(bytes: Data([2]))

        XCTAssertTrue(store.importedSourceNames.isEmpty)
        XCTAssertEqual(store.documentBlocks, client.blocks)
    }

    func testPagesCardsNameTheirSourceOnlyOnceThereIsMoreThanOne() {
        let (_, single) = opened(blocks: [DocumentBlock(source: .base, part: nil, start: 0, count: 2)], pages: 2)
        XCTAssertEqual(single.pageSourceNames(baseName: "a.pdf"), [nil, nil])

        let (_, mixed) = opened()
        XCTAssertEqual(
            mixed.pageSourceNames(baseName: "a.pdf"),
            ["a.pdf", "a.pdf", "Blank page", "Imported PDF", "Imported PDF", "Imported PDF"]
        )
    }
}
