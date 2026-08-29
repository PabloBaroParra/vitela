import XCTest
@testable import Vitela

final class ViewerStoreSelectionTests: XCTestCase {
    func testPageCharactersFetchIsCachedAfterApplyAndNotRefetched() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        client.pageCharactersByPage[0] = FakePageCharacters()

        let request = try XCTUnwrap(store.beginPageCharactersFetch(page: 0))
        store.applyPageCharacters(store.pageCharactersResult(for: request), for: request)

        XCTAssertNil(store.beginPageCharactersFetch(page: 0), "an already-cached page must not be refetched")
        XCTAssertEqual(client.pageCharactersRequests, [0])
    }

    func testStaleFetchCompletionIsDroppedAfterANewerDocumentOpens() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let staleRequest = try XCTUnwrap(store.beginPageCharactersFetch(page: 0))
        store.open(bytes: Data([2]))

        store.applyPageCharacters(FakePageCharacters(), for: staleRequest)

        XCTAssertNotNil(
            store.beginPageCharactersFetch(page: 0),
            "a stale completion must not populate the new document's cache"
        )
    }

    func testBeginSelectionStartsAnEmptySelectionAtTheNearestCaret() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        client.pageCharactersByPage[0] = FakePageCharacters(caretAt: { _, _ in 5 })
        let request = try XCTUnwrap(store.beginPageCharactersFetch(page: 0))
        store.applyPageCharacters(store.pageCharactersResult(for: request), for: request)

        store.beginSelection(page: 0, xPt: 10, yPt: 20)

        XCTAssertEqual(store.selection, TextSelection(page: 0, anchor: 5, focus: 5))
        XCTAssertNil(store.selectedText, "an anchor == focus selection has no text — a plain click, not a drag")
    }

    func testExtendSelectionMovesFocusButKeepsTheOriginalAnchor() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        var nextCaret = 5
        let characters = FakePageCharacters(
            caretAt: { _, _ in nextCaret },
            textIn: { anchor, focus in "\(anchor)-\(focus)" }
        )
        client.pageCharactersByPage[0] = characters
        let request = try XCTUnwrap(store.beginPageCharactersFetch(page: 0))
        store.applyPageCharacters(store.pageCharactersResult(for: request), for: request)
        store.beginSelection(page: 0, xPt: 0, yPt: 0)

        nextCaret = 12
        store.extendSelection(page: 0, xPt: 50, yPt: 50)

        XCTAssertEqual(store.selection, TextSelection(page: 0, anchor: 5, focus: 12))
        XCTAssertEqual(store.selectedText, "5-12")
    }

    func testExtendSelectionOnADifferentPageIsANoOp() throws {
        let client = FakePdfCoreClient(
            pages: [PageDimensions(width: 612, height: 792), PageDimensions(width: 612, height: 792)]
        )
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let characters = FakePageCharacters(caretAt: { _, _ in 3 })
        client.pageCharactersByPage[0] = characters
        client.pageCharactersByPage[1] = characters
        for page in [0, 1] {
            let request = try XCTUnwrap(store.beginPageCharactersFetch(page: page))
            store.applyPageCharacters(store.pageCharactersResult(for: request), for: request)
        }
        store.beginSelection(page: 0, xPt: 0, yPt: 0)

        store.extendSelection(page: 1, xPt: 0, yPt: 0)

        XCTAssertEqual(store.selection?.page, 0, "a drag that crosses onto another page must not move the selection there")
    }

    func testClearSelectionRemovesIt() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        client.pageCharactersByPage[0] = FakePageCharacters(caretAt: { _, _ in 1 })
        let request = try XCTUnwrap(store.beginPageCharactersFetch(page: 0))
        store.applyPageCharacters(store.pageCharactersResult(for: request), for: request)
        store.beginSelection(page: 0, xPt: 0, yPt: 0)

        store.clearSelection()

        XCTAssertNil(store.selection)
    }

    func testSelectionRectsAreEmptyForAnotherPage() throws {
        let client = FakePdfCoreClient(
            pages: [PageDimensions(width: 612, height: 792), PageDimensions(width: 612, height: 792)]
        )
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let rect = TextRect(xPt: 1, yPt: 2, widthPt: 3, heightPt: 4)
        let characters = FakePageCharacters(caretAt: { _, _ in 0 }, rectsIn: { _, _ in [rect] })
        client.pageCharactersByPage[0] = characters
        let request = try XCTUnwrap(store.beginPageCharactersFetch(page: 0))
        store.applyPageCharacters(store.pageCharactersResult(for: request), for: request)
        store.beginSelection(page: 0, xPt: 0, yPt: 0)

        XCTAssertEqual(store.selectionRects(forPage: 0), [rect])
        XCTAssertEqual(store.selectionRects(forPage: 1), [])
    }

    func testOpeningANewDocumentClearsTheCacheAndSelection() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        client.pageCharactersByPage[0] = FakePageCharacters(caretAt: { _, _ in 1 })
        let request = try XCTUnwrap(store.beginPageCharactersFetch(page: 0))
        store.applyPageCharacters(store.pageCharactersResult(for: request), for: request)
        store.beginSelection(page: 0, xPt: 0, yPt: 0)

        store.open(bytes: Data([2]))

        XCTAssertNil(store.selection)
        XCTAssertNotNil(store.beginPageCharactersFetch(page: 0), "a fresh document must refetch its own page characters")
    }
}
