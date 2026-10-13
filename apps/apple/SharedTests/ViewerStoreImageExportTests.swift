import XCTest
@testable import Vitela

final class ViewerStoreImageExportTests: XCTestCase {
    private func openStore(_ client: FakePdfCoreClient) -> ViewerStore {
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return store
    }

    private func pages(_ count: Int = 3) -> FakePdfCoreClient {
        FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: count))
    }

    func testAllPagesAreNamedByTheCore() throws {
        let store = openStore(pages())

        let plan = try store.planImageExport(pages: .all, dpi: 150, format: .jpeg, documentName: "report.pdf").get()

        XCTAssertEqual(plan.files.map(\.fileName), ["report-1.jpg", "report-2.jpg", "report-3.jpg"])
        XCTAssertEqual(plan.dpi, 150)
    }

    func testATypedRangePicksItsPages() throws {
        let store = openStore(pages())

        let plan = try store.planImageExport(pages: .range("1,3"), dpi: 72, format: .png, documentName: "a.pdf").get()

        XCTAssertEqual(plan.files.map(\.page), [0, 2])
    }

    func testResolutionIsCheckedFirstAndUsesWindowsLimits() {
        let store = ViewerStore(client: pages())

        XCTAssertEqual(
            store.planImageExport(pages: .all, dpi: 401, format: .png, documentName: "a.pdf"),
            .failure(.failed("Choose a resolution between 72 and 400 DPI."))
        )
        XCTAssertEqual(
            store.planImageExport(pages: .all, dpi: 71, format: .png, documentName: "a.pdf"),
            .failure(.failed("Choose a resolution between 72 and 400 DPI."))
        )
    }

    func testAnOversizedPageIsNamedBeforeAnythingIsWritten() {
        let client = pages()
        client.oversizedPage = 1
        let store = openStore(client)

        XCTAssertEqual(
            store.planImageExport(pages: .all, dpi: 400, format: .png, documentName: "a.pdf"),
            .failure(.failed("Page 2 is too large to export at 400 DPI. Choose a lower resolution."))
        )
    }

    func testARefusingDocumentSaysSo() {
        let client = pages()
        client.textExtractionPermitted = false

        XCTAssertEqual(
            openStore(client).planImageExport(pages: .all, dpi: 150, format: .png, documentName: "a.pdf"),
            .failure(.notPermitted("This document does not permit extracting its pages as images."))
        )
    }

    func testABadRangeReturnsTheCoresSentence() {
        XCTAssertEqual(
            openStore(pages()).planImageExport(pages: .range("9"), dpi: 150, format: .png, documentName: "a.pdf"),
            .failure(.failed("\"9\" is not a page range in this document."))
        )
    }

    func testTheSummaryMatchesWindows() {
        XCTAssertEqual(ViewerStore.imageExportSummary(count: 1, format: .jpeg, folder: "/tmp"), "Exported 1 page as JPEG to /tmp.")
        XCTAssertEqual(ViewerStore.imageExportSummary(count: 2, format: .png, folder: "/tmp"), "Exported 2 pages as PNG to /tmp.")
    }
}

extension ViewerStoreImageExportTests {
    func testCurrentPageIsThePageInView() throws {
        let client = FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: 3))
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        store.goToPage(2)

        let plan = try store.planImageExport(pages: .current, dpi: 150, format: .png, documentName: "a.pdf").get()

        XCTAssertEqual(plan.files.map(\.page), [2])
    }
}

/// Pages are drawn from one output snapshot, never the open document: its
/// preview leaves this session's annotations out for the overlay to paint,
/// so an unsaved highlight would be missing from every image.
extension ViewerStoreImageExportTests {
    func testEveryPageIsDrawnFromOneOutputSnapshotNotTheOpenDocument() throws {
        let client = pages()
        let store = openStore(client)
        let plan = try store.planImageExport(pages: .all, dpi: 150, format: .png, documentName: "a.pdf").get()

        let request = try XCTUnwrap(store.beginImageExport(plan))
        let source = try store.imageExportSource(for: request).get()
        let images = try plan.files.map { try store.imageExportResult($0, from: source).get() }

        XCTAssertEqual(client.snapshotsTaken, 1)
        XCTAssertEqual(client.exportedFromLive, [])
        XCTAssertEqual(client.exportedImages.map(\.page), [0, 1, 2])
        XCTAssertEqual(images, [Data("image-0".utf8), Data("image-1".utf8), Data("image-2".utf8)])
    }

    /// Never a fallback to the open document: that is the image without the edits.
    func testASnapshotThatFailsExportsNothingAndSaysWhy() throws {
        let client = pages()
        client.snapshotError = EditFailure.failed("The document could not be processed.")
        let store = openStore(client)
        let plan = try store.planImageExport(pages: .all, dpi: 150, format: .png, documentName: "a.pdf").get()

        let request = try XCTUnwrap(store.beginImageExport(plan))

        XCTAssertEqual(store.imageExportSource(for: request).map(\.plan), .failure(.failed("The document could not be processed.")))
        XCTAssertEqual(client.exportedFromLive, [])
        XCTAssertTrue(client.exportedImages.isEmpty)
    }

    func testThereIsNothingToExportWithoutADocument() {
        let store = ViewerStore(client: pages())

        XCTAssertNil(store.beginImageExport(ViewerStore.ImageExportPlan(files: [], dpi: 150, format: .png)))
    }
}
