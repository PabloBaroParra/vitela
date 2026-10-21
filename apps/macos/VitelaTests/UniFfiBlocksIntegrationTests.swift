import XCTest
@testable import Vitela

/// Organize's Documents view against the real core: an imported PDF becomes
/// its own block, named by its file, and a block moves and deletes whole.
final class UniFfiBlocksIntegrationTests: XCTestCase {
    func testAnImportIsItsOwnNamedBlockThatMovesAndDeletesWhole() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let count = store.pageSlots.count
        XCTAssertEqual(store.documentBlocks, [DocumentBlock(source: .base, part: nil, start: 0, count: count)])

        let source = try store.prepareImport(bytes: try Self.sampleBytes(), password: nil).get()
        let importing = try XCTUnwrap(store.beginImport([source], names: ["copy.pdf"]))
        store.applyPageWork(store.pageWorkResult(for: importing), for: importing)

        XCTAssertEqual(store.documentBlockRows(baseName: "sample.pdf").map(\.title), ["sample.pdf", "copy.pdf"])

        let moving = try XCTUnwrap(store.beginBlockEdit(1, .move(toSlot: 0)))
        store.applyPageWork(store.pageWorkResult(for: moving), for: moving)
        XCTAssertEqual(store.documentBlockRows(baseName: "sample.pdf").map(\.title), ["copy.pdf", "sample.pdf"])
        XCTAssertEqual(store.documentBlocks.map(\.start), [0, count])

        let deleting = try XCTUnwrap(store.beginBlockEdit(0, .delete))
        store.applyPageWork(store.pageWorkResult(for: deleting), for: deleting)
        XCTAssertEqual(store.documentBlockRows(baseName: "sample.pdf").map(\.title), ["sample.pdf"])
        XCTAssertEqual(store.pageSlots.count, count)
    } }
}
