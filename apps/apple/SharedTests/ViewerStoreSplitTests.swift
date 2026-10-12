import XCTest
@testable import Vitela

final class ViewerStoreSplitTests: XCTestCase {
    private func openStore(_ client: FakePdfCoreClient) -> ViewerStore {
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return store
    }

    private func pages(_ count: Int) -> FakePdfCoreClient {
        FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: count))
    }

    func testCutPointsBecomePartsNamedAfterTheDocument() throws {
        let store = openStore(pages(8))

        let plan = try store.planSplit(cuts: "3,7", documentName: "report.pdf").get()

        XCTAssertEqual(plan.parts, [
            SplitPart(first: 0, last: 2, fileName: "report-1.pdf"),
            SplitPart(first: 3, last: 6, fileName: "report-2.pdf"),
            SplitPart(first: 7, last: 7, fileName: "report-3.pdf"),
        ])
    }

    func testAOnePageDocumentCannotBeSplit() {
        XCTAssertEqual(
            openStore(pages(1)).planSplit(cuts: "1", documentName: "a.pdf"),
            .failure(.failed("A document of one page cannot be split."))
        )
    }

    func testGatesComeBeforeTheCutsAndUseSplitWording() {
        let client = pages(4)
        client.fullRewritePermitted = false
        let store = openStore(client)

        XCTAssertEqual(store.planSplit(cuts: "nonsense", documentName: "a.pdf"), .failure(.notPermitted(
            "Splitting pages rewrites the whole file, which this document's encryption or available credentials do not allow."
        )))
    }

    func testAPartIsAnExtractionOfItsPages() throws {
        let client = pages(5)
        let store = openStore(client)
        let plan = try store.planSplit(cuts: "2", documentName: "a.pdf").get()

        let request = try XCTUnwrap(store.beginSplitPart(plan.parts[1], of: plan))
        _ = store.extractResult(for: request)

        XCTAssertEqual(client.extractedPages, [[2, 3, 4]])
    }

    func testWordingMatchesWindows() {
        XCTAssertEqual(
            ViewerStore.splitSummary(written: 3, folder: "/tmp/out", sourceIsSigned: true),
            "Split into 3 PDFs in /tmp/out. The original document is signed, so the signature in each part no longer verifies."
        )
        XCTAssertEqual(
            ViewerStore.splitStopped(written: 1, part: "a-2.pdf", reason: "Disk full."),
            "Split stopped after 1 written: a-2.pdf could not be written: Disk full."
        )
    }
}
