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
        let request = try XCTUnwrap(store.beginImageExport(try XCTUnwrap(plan.files.first), of: plan, from: try XCTUnwrap(store.document)))
        let png = try store.imageExportResult(for: request).get()
        XCTAssertEqual(Array(png.prefix(8)), [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])

        let jpegPlan = try store.planImageExport(pages: .all, dpi: 72, format: .jpeg, documentName: "report.pdf").get()
        let jpegRequest = try XCTUnwrap(store.beginImageExport(try XCTUnwrap(jpegPlan.files.first), of: jpegPlan, from: try XCTUnwrap(store.document)))
        XCTAssertEqual(Array(try store.imageExportResult(for: jpegRequest).get().prefix(2)), [0xFF, 0xD8])
    } }

    func testAnUnsavedAnnotationReachesTheExportedImage() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let plan = try store.planImageExport(pages: .range("1"), dpi: 72, format: .png, documentName: "report.pdf").get()
        let file = try XCTUnwrap(plan.files.first)
        func exported() throws -> Data {
            let output = try store.outputDocument(for: try XCTUnwrap(store.beginOutput())).get()
            return try store.imageExportResult(for: try XCTUnwrap(store.beginImageExport(file, of: plan, from: output))).get()
        }
        let before = try exported()

        store.applyAnnotationEdit(.add(kind: .highlight, page: 0, rect: PdfRect(x: 72, y: 72, width: 300, height: 200), color: PdfColor(r: 255, g: 0, b: 0)))
        let annotated = try exported()
        let live = try store.imageExportResult(for: try XCTUnwrap(store.beginImageExport(file, of: plan, from: try XCTUnwrap(store.document)))).get()

        XCTAssertNotEqual(annotated, before, "the snapshot draws the highlight")
        XCTAssertEqual(live, before, "the live handle leaves it to the overlay, which is why export cannot use it")
        XCTAssertEqual(store.annotations.count, 1, "taking the snapshot leaves the session's annotation editable")
    } }

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

extension UniFfiOutputIntegrationTests {
    /// A line the sample paints is retyped against the real core, through
    /// the same page-work pipeline the shell runs, and reads back changed.
    func testRetypingALineOfTheSampleChangesWhatThePageSays() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        XCTAssertTrue(store.editingAllowed)
        store.setContentEditMode(true)
        func read() throws -> PageContent {
            let request = try XCTUnwrap(store.beginPageContent(page: 0))
            store.applyPageContent(store.pageContentResult(for: request), for: request)
            return try XCTUnwrap(store.pageContent[0])
        }
        let content = try read()
        let line = try XCTUnwrap(content.textRuns.first { !$0.substitutesFont && $0.text.count > 3 }, "the sample paints no simple-font line")
        // A prefix of the line: every character is one its font already shows.
        let shorter = String(line.text.prefix(3))

        let request = try XCTUnwrap(store.beginContentEdit(.retypeText(line, to: shorter)))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertTrue(store.canUndo)
        XCTAssertTrue(try read().textRuns.contains { $0.text == shorter })
    } }
}
