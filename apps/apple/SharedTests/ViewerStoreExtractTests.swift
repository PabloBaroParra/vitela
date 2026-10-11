import XCTest
@testable import Vitela

final class ViewerStoreExtractTests: XCTestCase {
    private func openStore(_ client: FakePdfCoreClient) -> ViewerStore {
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return store
    }

    private func threePages() -> FakePdfCoreClient {
        FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: 3))
    }

    func testATypedRangeBecomesZeroBasedPages() {
        let store = openStore(threePages())

        XCTAssertEqual(try store.planExtract(range: "1,3").get(), ViewerStore.ExtractPlan(pages: [0, 2], sourceIsSigned: false))
    }

    func testTheCoresOwnSentenceExplainsABadRange() {
        let store = openStore(threePages())

        XCTAssertEqual(store.planExtract(range: "2-9"), .failure(.failed("\"2-9\" is not a page range in this document.")))
    }

    func testABlankRangeAsksForOne() {
        let store = openStore(threePages())

        XCTAssertEqual(store.planExtract(range: "  "), .failure(.failed("Type which pages to extract, for example 1-3,7.")))
    }

    /// Both gates come before the grammar: a reader must learn the document
    /// refuses before they type anything.
    func testPermissionGatesComeBeforeTheRange() {
        let client = threePages()
        client.textExtractionPermitted = false
        let store = openStore(client)
        XCTAssertEqual(store.planExtract(range: "nonsense"), .failure(.notPermitted("This document does not permit extracting its pages.")))

        client.textExtractionPermitted = true
        client.fullRewritePermitted = false
        XCTAssertEqual(store.planExtract(range: "1"), .failure(.notPermitted(
            "Extracting pages rewrites the whole file, which this document's encryption or available credentials do not allow."
        )))
    }

    func testNoDocumentHasNothingToExtract() {
        let store = ViewerStore(client: threePages())

        XCTAssertNil(store.beginExtract(ViewerStore.ExtractPlan(pages: [0], sourceIsSigned: false)))
        XCTAssertEqual(store.planExtract(range: "1"), .failure(.failed("The document is no longer open.")))
    }

    func testExtractingLeavesTheOpenDocumentUntouched() throws {
        let client = threePages()
        let store = openStore(client)
        let plan = try store.planExtract(range: "2-3").get()

        let request = try XCTUnwrap(store.beginExtract(plan))
        XCTAssertEqual(try store.extractResult(for: request).get(), client.extractedBytes)

        XCTAssertEqual(client.extractedPages, [[1, 2]])
        XCTAssertFalse(store.hasUnsavedChanges)
        XCTAssertEqual(store.pageSlots.count, 3)
    }

    func testTheSummaryMatchesWindowsWording() {
        XCTAssertEqual(
            ViewerStore.extractSummary(count: 1, destination: "/tmp/a.pdf", sourceIsSigned: false),
            "Extracted 1 page to /tmp/a.pdf."
        )
        XCTAssertEqual(
            ViewerStore.extractSummary(count: 3, destination: "/tmp/a.pdf", sourceIsSigned: true),
            "Extracted 3 pages to /tmp/a.pdf. The original document is signed, so the extracted PDF's signature no longer verifies."
        )
    }
}
