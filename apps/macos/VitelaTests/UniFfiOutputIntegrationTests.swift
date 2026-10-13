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
        let request = try XCTUnwrap(store.beginImageExport(try XCTUnwrap(plan.files.first), of: plan))
        let png = try store.imageExportResult(for: request).get()
        XCTAssertEqual(Array(png.prefix(8)), [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])

        let jpegPlan = try store.planImageExport(pages: .all, dpi: 72, format: .jpeg, documentName: "report.pdf").get()
        let jpegRequest = try XCTUnwrap(store.beginImageExport(try XCTUnwrap(jpegPlan.files.first), of: jpegPlan))
        XCTAssertEqual(Array(try store.imageExportResult(for: jpegRequest).get().prefix(2)), [0xFF, 0xD8])
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

extension UniFfiOutputIntegrationTests {
    /// A line of the sample moved against the real core lands where it was
    /// dropped, and is refused for retyping until the document is saved.
    func testMovingALineOfTheSampleMovesItAndHoldsItsRetype() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        store.setContentEditMode(true)
        func read() throws -> PageContent {
            let request = try XCTUnwrap(store.beginPageContent(page: 0))
            store.applyPageContent(store.pageContentResult(for: request), for: request)
            return try XCTUnwrap(store.pageContent[0])
        }
        let line = try XCTUnwrap(try read().textRuns.first { !$0.substitutesFont })
        let to = PdfRect(x: line.bbox.x + 20, y: line.bbox.y - 30, width: line.bbox.width, height: line.bbox.height)

        let request = try XCTUnwrap(store.beginContentEdit(.moveText(line, to: to)))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertTrue(store.canUndo)
        let moved = try XCTUnwrap(try read().textRuns.first { $0.id == line.id })
        XCTAssertEqual(moved.bbox.x, to.x, accuracy: 1)
        XCTAssertEqual(moved.bbox.y, to.y, accuracy: 1)
        XCTAssertTrue(store.client.textRunHasPendingMove(document: try XCTUnwrap(store.document), run: moved))
    } }
}

extension UniFfiOutputIntegrationTests {
    /// A new line added against the real core is painted by the page: it
    /// reads back as one of the page's own runs, in its own Helvetica.
    func testAddingALineToTheSampleMakesItPartOfThePage() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        store.setContentEditMode(true)

        let (problem, request) = store.beginTextInsert(page: 0, at: PdfPoint(x: 72, y: 120), text: "Vitela was here", size: "14")
        XCTAssertNil(problem)
        let work = try XCTUnwrap(request)
        store.applyPageWork(store.pageWorkResult(for: work), for: work)
        XCTAssertTrue(store.canUndo)

        let read = try XCTUnwrap(store.beginPageContent(page: 0))
        store.applyPageContent(store.pageContentResult(for: read), for: read)
        let added = try XCTUnwrap(store.pageContent[0]?.textRuns.first { $0.text == "Vitela was here" })
        XCTAssertEqual(added.fontKind, .standard14)
        XCTAssertTrue(added.resourceFontName.hasPrefix("VitelaText"))
    } }
}

extension UniFfiOutputIntegrationTests {
    /// One page painting one 1×1 red image at (72, 600), 100 × 50 points.
    static func onePhotoPdf() -> Data {
        var pdf = Data("%PDF-1.7\n".utf8)
        var offsets: [Int] = []
        func object(_ body: Data) {
            offsets.append(pdf.count)
            pdf.append(Data("\(offsets.count) 0 obj\n".utf8))
            pdf.append(body)
            pdf.append(Data("\nendobj\n".utf8))
        }
        let content = Data("q 100 0 0 50 72 600 cm /Im1 Do Q".utf8)
        object(Data("<< /Type /Catalog /Pages 2 0 R >>".utf8))
        object(Data("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".utf8))
        object(Data("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>".utf8))
        object(Data("<< /Length \(content.count) >>\nstream\n".utf8) + content + Data("\nendstream".utf8))
        object(Data("<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 3 >>\nstream\n".utf8)
            + Data([0xFF, 0x00, 0x00]) + Data("\nendstream".utf8))
        let xref = pdf.count
        pdf.append(Data("xref\n0 \(offsets.count + 1)\n0000000000 65535 f \n".utf8))
        for offset in offsets { pdf.append(Data(String(format: "%010d 00000 n \n", offset).utf8)) }
        pdf.append(Data("trailer\n<< /Size \(offsets.count + 1) /Root 1 0 R >>\nstartxref\n\(xref)\n%%EOF\n".utf8))
        return pdf
    }

    /// An image the page paints is found, moved, resized and deleted against
    /// the real core, each as one undoable step that reads back.
    func testAnImageIsMovedResizedAndDeleted() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: Self.onePhotoPdf())
        XCTAssertEqual(store.state, .loaded)
        store.setContentEditMode(true)
        func read() throws -> [ContentImage] {
            let request = try XCTUnwrap(store.beginPageContent(page: 0))
            store.applyPageContent(store.pageContentResult(for: request), for: request)
            return try XCTUnwrap(store.pageContent[0]).images
        }
        func perform(_ edit: ContentEdit) throws {
            let request = try XCTUnwrap(store.beginContentEdit(edit))
            store.applyPageWork(store.pageWorkResult(for: request), for: request)
        }
        let photo = try XCTUnwrap(try read().first)
        XCTAssertEqual(photo.bbox.x, 72, accuracy: 0.5)
        XCTAssertEqual(photo.bbox.width, 100, accuracy: 0.5)

        try perform(.moveImage(photo, to: PdfRect(x: 172, y: 500, width: 100, height: 50)))
        XCTAssertEqual(try XCTUnwrap(try read().first).bbox.x, 172, accuracy: 0.5)
        XCTAssertTrue(store.canUndo)

        store.undo()
        if let refresh = store.beginPageRefresh() { store.applyPageWork(store.pageWorkResult(for: refresh), for: refresh) }
        let back = try XCTUnwrap(try read().first)
        XCTAssertEqual(back.bbox.x, 72, accuracy: 0.5)

        try perform(.resizeImage(back, to: PdfRect(x: 72, y: 600, width: 200, height: 100)))
        XCTAssertEqual(try XCTUnwrap(try read().first).bbox.width, 200, accuracy: 0.5)

        store.undo()
        if let refresh = store.beginPageRefresh() { store.applyPageWork(store.pageWorkResult(for: refresh), for: refresh) }
        try perform(.removeImage(try XCTUnwrap(try read().first)))
        XCTAssertEqual(try read(), [])
    } }
}

