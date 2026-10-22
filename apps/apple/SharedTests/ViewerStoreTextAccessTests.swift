import XCTest
@testable import Vitela

/// A document that withholds its text gets no search, no selection and no
/// copy, and is told why in Linux's and Windows' words — before any work is
/// queued or a match count could show what it withholds.
final class ViewerStoreTextAccessTests: XCTestCase {
    private func store(permitted: Bool) -> (FakePdfCoreClient, ViewerStore) {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.textExtractionPermitted = permitted
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return (client, store)
    }

    func testTheRefusalIsTheOtherShellsSentence() {
        XCTAssertEqual(ViewerStore.textExtractionRefusalMessage, "This document does not permit copying or extracting its text.")
    }

    func testSearchIsRefusedBeforeAnythingIsQueued() {
        let (client, store) = store(permitted: false)
        XCTAssertNil(store.beginSearch(query: "Vitela"))
        XCTAssertEqual(store.searchStatus, ViewerStore.textExtractionRefusalMessage)
        XCTAssertNil(store.search)
        XCTAssertTrue(client.searchedQueries.isEmpty)
    }

    func testNoCharactersAreLoadedAndADragSaysWhyNothingSelects() {
        let (client, store) = store(permitted: false)
        XCTAssertNil(store.beginPageCharactersFetch(page: 0))
        store.beginSelection(page: 0, xPt: 10, yPt: 10)
        XCTAssertNil(store.selection)
        XCTAssertEqual(store.editStatus, ViewerStore.textExtractionRefusalMessage)
        XCTAssertTrue(client.pageCharactersRequests.isEmpty)
    }

    func testAPermittedDocumentSearchesAndLoadsCharactersAsBefore() {
        let (_, store) = store(permitted: true)
        XCTAssertNil(store.textExtractionRefusal)
        XCTAssertNotNil(store.beginSearch(query: "Vitela"))
        XCTAssertNotNil(store.beginPageCharactersFetch(page: 0))
    }
}
