import XCTest
@testable import Vitela

/// What the reader writes out against the real core: extracted, split,
/// exported, compressed, protected and new documents.
final class UniFfiOutputIntegrationTests: XCTestCase {
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
        let png = try Self.firstPageImage(of: store, plan)
        XCTAssertEqual(Array(png.prefix(8)), [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])

        let jpegPlan = try store.planImageExport(pages: .all, dpi: 72, format: .jpeg, documentName: "report.pdf").get()
        XCTAssertEqual(Array(try Self.firstPageImage(of: store, jpegPlan).prefix(2)), [0xFF, 0xD8])
    } }

    /// The open handle draws its preview, which leaves this session's
    /// annotations out for the overlay to paint; an export has no overlay.
    func testAnExportedPageCarriesAnUnsavedInkStroke() throws { try offMain {
        let untouched = try Self.openSample()
        let annotated = try Self.openSampleWithAnInkStroke()
        let plan = try untouched.planImageExport(pages: .range("1"), dpi: 72, format: .png, documentName: "a.pdf").get()

        XCTAssertNotEqual(try Self.firstPageImage(of: annotated, plan), try Self.firstPageImage(of: untouched, plan))
    } }

    /// Print rasterizes straight to paper, so it needs the snapshot too.
    func testAPrintedPageCarriesAnUnsavedInkStroke() throws { try offMain {
        let untouched = try Self.openSample()
        let annotated = try Self.openSampleWithAnInkStroke()

        let printed = try XCTUnwrap(annotated.printPage(0, for: try XCTUnwrap(annotated.beginPrint()), dpi: 72))
        let plain = try XCTUnwrap(untouched.printPage(0, for: try XCTUnwrap(untouched.beginPrint()), dpi: 72))
        XCTAssertNotEqual(printed.rgba, plain.rgba)
    } }

    private static func openSample() throws -> ViewerStore {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try sampleBytes())
        return store
    }

    /// Unsaved: the stroke lives only in the session's annotation list.
    private static func openSampleWithAnInkStroke() throws -> ViewerStore {
        let store = try openSample()
        store.armAnnotationTool(.ink)
        XCTAssertTrue(store.beginAnnotationDraft(page: 0, at: PdfPoint(x: 100, y: 100)))
        store.continueAnnotationDraft(page: 0, to: PdfPoint(x: 300, y: 400))
        store.endAnnotationDraft(page: 0, at: PdfPoint(x: 500, y: 100))
        XCTAssertEqual(store.annotations.count, 1)
        return store
    }

    private static func firstPageImage(of store: ViewerStore, _ plan: ViewerStore.ImageExportPlan) throws -> Data {
        let source = try store.imageExportSource(for: try XCTUnwrap(store.beginImageExport(plan))).get()
        return try store.imageExportResult(try XCTUnwrap(plan.files.first), from: source).get()
    }

    func testCompressingNeverGrowsTheFile() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        XCTAssertNil(store.compressionRefusal())

        let request = try XCTUnwrap(store.beginCompress(.lossless))
        XCTAssertEqual(try store.compressionWillInvalidateSignatures(for: request).get(), false)
        let result = try store.compressResult(for: request, acknowledgingSignatureLoss: false).get()

        XCTAssertLessThanOrEqual(result.afterBytes, result.beforeBytes)
        XCTAssertEqual(result.savedBytes, result.beforeBytes - result.afterBytes)
        if result.reduced {
            let reopened = ViewerStore(client: UniFfiPdfCoreClient())
            reopened.open(bytes: result.bytes)
            XCTAssertEqual(reopened.pageSlots.count, store.pageSlots.count)
        }
    } }

    func testAProtectedCopyNeedsItsPasswordAndReopensEditable() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let request = try XCTUnwrap(store.beginProtect(openPassword: "open-pass", permissionsPassword: "owner-pass"))
        XCTAssertEqual(try store.protectionWillInvalidateSignatures(for: request).get(), false)

        let bytes = try store.protectResult(for: request, acknowledgingSignatureLoss: false).get()

        let locked = ViewerStore(client: UniFfiPdfCoreClient())
        locked.open(bytes: bytes)
        XCTAssertEqual(locked.state, .error(.passwordRequired), "the copy must ask for its password")
        XCTAssertTrue(store.reopenProtected(bytes, for: request))
        XCTAssertEqual(store.state, .loaded)
        XCTAssertTrue(store.editingAllowed, "opened with the permissions password, the copy stays editable")
    } }

    func testANewDocumentIsOneA4PageThatCanBeSaved() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.openBlank()

        XCTAssertEqual(store.state, .loaded)
        XCTAssertEqual(store.pageSlots.count, 1)
        let page = try XCTUnwrap(store.pageSlots.first?.dimensions)
        XCTAssertEqual(page.width, 595, accuracy: 1)
        XCTAssertEqual(page.height, 842, accuracy: 1)
        XCTAssertFalse(store.canUndo)

        let save = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        let reopened = ViewerStore(client: UniFfiPdfCoreClient())
        reopened.open(bytes: try store.saveResult(for: save).get())
        XCTAssertEqual(reopened.pageSlots.count, 1)
    } }
}
