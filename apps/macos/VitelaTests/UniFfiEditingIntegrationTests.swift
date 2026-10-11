import XCTest
@testable import Vitela

/// Runs the editing path against the real core through the generated
/// bindings — the fakes elsewhere cannot catch a mapping mistake between
/// `DocumentInfo` and `FfiDocumentInfo`. The test host is the app bundle, so
/// its `Contents/Frameworks` carries both dylibs.
///
/// Each test body runs on a background-QoS queue while the main thread waits
/// on an expectation (which spins the run loop rather than blocking). Every
/// core call waits on PDFium's render thread; waiting on it from a thread of
/// higher QoS trips Xcode's Thread Performance Checker, and symbolicating its
/// report has been seen to hang the whole test run. The lowest QoS cannot be
/// inverted. The store has no observers here, so leaving the main thread
/// changes nothing it does.
final class UniFfiEditingIntegrationTests: XCTestCase {
    private func offMain(_ body: @escaping () throws -> Void) throws {
        let done = expectation(description: "test body finished")
        var failure: Error?
        DispatchQueue.global(qos: .background).async {
            do { try body() } catch { failure = error }
            done.fulfill()
        }
        wait(for: [done], timeout: 30)
        if let failure { throw failure }
    }

    private static func sampleBytes() throws -> Data {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "vitela-sample", withExtension: "pdf"))
        return try Data(contentsOf: url)
    }

    func testPropertiesSurviveASaveAndReopen() throws { try offMain {
        let client = UniFfiPdfCoreClient()
        let store = ViewerStore(client: client)
        store.open(bytes: try Self.sampleBytes())
        XCTAssertEqual(store.state, .loaded)
        XCTAssertTrue(store.editingAllowed)

        store.setDocumentProperty(.title, to: "Vitela integration")
        store.setDocumentProperty(.keywords, to: "ñandú, café")
        XCTAssertTrue(store.setDocumentDate(.created, to: "2021-06-15 08:30:00"))
        XCTAssertTrue(store.canUndo)

        let request = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        let bytes = try store.saveResult(for: request).get()

        let reopened = ViewerStore(client: client)
        reopened.open(bytes: bytes)
        XCTAssertEqual(reopened.documentInfo?.title, "Vitela integration")
        XCTAssertEqual(reopened.documentInfo?.keywords, "ñandú, café")
        XCTAssertEqual(MetadataDateText.format(reopened.documentInfo?.creationDate), "2021-06-15 08:30:00")
    } }

    func testUndoRestoresTheFilesOwnValue() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let original = store.documentInfo

        store.setDocumentProperty(.author, to: "Someone else")
        store.undo()

        XCTAssertEqual(store.documentInfo, original)
        XCTAssertFalse(store.canUndo)
        XCTAssertTrue(store.canRedo)
    } }

    func testExtractingTheFirstPageMakesAOnePagePdf() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        // Page 0 does not exist in a one-based range: the core says why.
        guard case let .failure(.failed(reason)) = store.planExtract(range: "0") else {
            return XCTFail("a range naming page 0 must be refused with the core's sentence")
        }
        XCTAssertFalse(reason.isEmpty)

        let plan = try store.planExtract(range: "1").get()
        let request = try XCTUnwrap(store.beginExtract(plan))
        let bytes = try store.extractResult(for: request).get()

        let extracted = ViewerStore(client: UniFfiPdfCoreClient())
        extracted.open(bytes: bytes)
        XCTAssertEqual(extracted.pageSlots.count, 1)
        XCTAssertEqual(extracted.pageSlots.first?.dimensions, store.pageSlots.first?.dimensions)
    } }

    func testSplittingMakesPartsThatTogetherHoldEveryPage() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let count = store.pageSlots.count
        try XCTSkipIf(count < 2, "the sample needs two pages to be split")

        let plan = try store.planSplit(cuts: "1", documentName: "sample.pdf").get()
        XCTAssertEqual(plan.parts.count, 2)
        XCTAssertNotEqual(plan.parts[0].fileName, plan.parts[1].fileName)

        var total = 0
        for part in plan.parts {
            let request = try XCTUnwrap(store.beginSplitPart(part, of: plan))
            let reopened = ViewerStore(client: UniFfiPdfCoreClient())
            reopened.open(bytes: try store.extractResult(for: request).get())
            total += reopened.pageSlots.count
        }
        XCTAssertEqual(total, count)
    } }

    func testExportedPagesAreRealImages() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())

        let plan = try store.planImageExport(pages: .range("1"), dpi: 72, format: .png, documentName: "report.pdf").get()
        XCTAssertEqual(plan.files.first?.fileName.hasSuffix(".png"), true)
        XCTAssertTrue(plan.files.first?.fileName.hasPrefix("report") == true)
        let request = try XCTUnwrap(store.beginImageExport(try XCTUnwrap(plan.files.first), of: plan))
        let png = try store.imageExportResult(for: request).get()
        XCTAssertEqual(Array(png.prefix(8)), [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])

        let jpegPlan = try store.planImageExport(pages: .all, dpi: 72, format: .jpeg, documentName: "report.pdf").get()
        let jpegRequest = try XCTUnwrap(store.beginImageExport(try XCTUnwrap(jpegPlan.files.first), of: jpegPlan))
        XCTAssertEqual(Array(try store.imageExportResult(for: jpegRequest).get().prefix(2)), [0xFF, 0xD8])
    } }
}
