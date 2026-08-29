import XCTest
@testable import Vitela

final class ViewerStoreSearchTests: XCTestCase {
    func testSearchWithMatchesPublishesSessionAndStatus() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let match = SearchMatch(pageIndex: 0, text: "hi", characterBounds: [TextRect(xPt: 10, yPt: 20, widthPt: 5, heightPt: 8)])
        client.searchResults["hi"] = [match]

        let request = try XCTUnwrap(store.beginSearch(query: "hi"))
        store.applySearch(store.searchResult(for: request), for: request)

        XCTAssertEqual(store.search, SearchSession(query: "hi", matches: [match], currentIndex: 0))
        XCTAssertEqual(store.searchStatus, "Match 1 of 1 for \"hi\".")
    }

    func testSearchWithNoMatchesClearsSessionWithStatus() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        let request = try XCTUnwrap(store.beginSearch(query: "nope"))
        store.applySearch(store.searchResult(for: request), for: request)

        XCTAssertNil(store.search)
        XCTAssertEqual(store.searchStatus, "No matches for \"nope\".")
    }

    func testEmptyQueryDoesNotBeginASearch() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        XCTAssertNil(store.beginSearch(query: ""))
        XCTAssertTrue(client.searchedQueries.isEmpty)
    }

    func testBeginSearchWithoutADocumentIsANoOp() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)

        XCTAssertNil(store.beginSearch(query: "hi"))
    }

    /// The document was replaced (or reopened) while a search ran: its
    /// matches would address pages that may no longer be on screen, or mean
    /// something different in the new document.
    func testStaleSearchCompletionDoesNotOverwriteANewerDocument() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let staleRequest = try XCTUnwrap(store.beginSearch(query: "hi"))
        store.open(bytes: Data([2]))

        store.applySearch(.success([SearchMatch(pageIndex: 0, text: "hi", characterBounds: [])]), for: staleRequest)

        XCTAssertNil(store.search)
    }

    func testStepSearchMatchWrapsAroundBothEnds() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let matches = [
            SearchMatch(pageIndex: 0, text: "a", characterBounds: []),
            SearchMatch(pageIndex: 1, text: "a", characterBounds: []),
        ]
        client.searchResults["a"] = matches
        let request = try XCTUnwrap(store.beginSearch(query: "a"))
        store.applySearch(store.searchResult(for: request), for: request)

        store.stepSearchMatch(by: 1)
        XCTAssertEqual(store.search?.currentIndex, 1)
        store.stepSearchMatch(by: 1)
        XCTAssertEqual(store.search?.currentIndex, 0, "stepping past the last match wraps to the first")
        store.stepSearchMatch(by: -1)
        XCTAssertEqual(store.search?.currentIndex, 1, "stepping before the first match wraps to the last")
    }

    func testStepSearchMatchWithoutASearchIsANoOp() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        store.stepSearchMatch(by: 1)

        XCTAssertNil(store.search)
    }

    func testSearchFailurePublishesTheUnderlyingMessage() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        client.searchError = TextQueryFailure.notPermitted

        let request = try XCTUnwrap(store.beginSearch(query: "hi"))
        store.applySearch(store.searchResult(for: request), for: request)

        XCTAssertNil(store.search)
        XCTAssertEqual(store.searchStatus, "Could not search: Text extraction is not permitted for this document.")
    }

    func testSearchMatchRectsExcludeTheCurrentMatchAndAreGroupedByPage() throws {
        let client = FakePdfCoreClient(
            pages: [PageDimensions(width: 612, height: 792), PageDimensions(width: 612, height: 792)]
        )
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let firstRect = TextRect(xPt: 1, yPt: 1, widthPt: 1, heightPt: 1)
        let secondRect = TextRect(xPt: 2, yPt: 2, widthPt: 2, heightPt: 2)
        let matches = [
            SearchMatch(pageIndex: 0, text: "a", characterBounds: [firstRect]),
            SearchMatch(pageIndex: 0, text: "a", characterBounds: [secondRect]),
        ]
        client.searchResults["a"] = matches
        let request = try XCTUnwrap(store.beginSearch(query: "a"))
        store.applySearch(store.searchResult(for: request), for: request)

        XCTAssertEqual(store.currentSearchMatchRects(forPage: 0), [firstRect])
        XCTAssertEqual(store.searchMatchRects(forPage: 0), [secondRect])
        XCTAssertEqual(store.searchMatchRects(forPage: 1), [])
    }
}
