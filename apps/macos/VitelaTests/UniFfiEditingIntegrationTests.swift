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

    /// A page edit only reaches the pixels once the render side is
    /// re-derived; this checks the layout the reader shows, and the saved file.
    func testARotatedPageIsShownAndSavedTurned() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let before = try XCTUnwrap(store.pageSlots.first?.dimensions)

        let request = try XCTUnwrap(store.beginPageEdit(.rotate(page: 0, degrees: 90)))
        let outcome = store.pageWorkResult(for: request)
        store.applyPageWork(outcome, for: request)

        XCTAssertEqual(store.editStatus, "Page rotated. Changes are pending save.")
        let turned = PageDimensions(width: before.height, height: before.width)
        XCTAssertEqual(store.pageSlots.first?.dimensions, turned)
        let render = try XCTUnwrap(store.beginRender(page: 0))
        let image = try store.renderResult(for: render).get()
        XCTAssertGreaterThan(image.width, image.height, "the preview must draw the turned page")

        let save = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        let reopened = ViewerStore(client: UniFfiPdfCoreClient())
        reopened.open(bytes: try store.saveResult(for: save).get())
        XCTAssertEqual(reopened.pageSlots.first?.dimensions, turned)

        store.undo()
        let refresh = try XCTUnwrap(store.beginPageRefresh())
        store.applyPageWork(store.pageWorkResult(for: refresh), for: refresh)
        XCTAssertEqual(store.pageSlots.first?.dimensions, before)
    } }

    func testADeletedPageIsGoneFromThePreviewAndTheSavedFile() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let count = store.pageSlots.count
        try XCTSkipIf(count < 2, "the sample needs two pages to delete one")

        let request = try XCTUnwrap(store.beginPageEdit(.remove(page: 0)))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
        XCTAssertEqual(store.pageSlots.count, count - 1)

        let save = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        let reopened = ViewerStore(client: UniFfiPdfCoreClient())
        reopened.open(bytes: try store.saveResult(for: save).get())
        XCTAssertEqual(reopened.pageSlots.count, count - 1)
    } }

    func testAnInsertedBlankPageIsA4InThePreviewAndTheSavedFile() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let count = store.pageSlots.count

        let request = try XCTUnwrap(store.beginPageEdit(.insertBlank(at: 0, orientation: .landscape)))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertEqual(store.pageSlots.count, count + 1)
        let blank = try XCTUnwrap(store.pageSlots.first?.dimensions)
        XCTAssertEqual(blank.width, 842, accuracy: 1)
        XCTAssertEqual(blank.height, 595, accuracy: 1)

        let save = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        let reopened = ViewerStore(client: UniFfiPdfCoreClient())
        reopened.open(bytes: try store.saveResult(for: save).get())
        XCTAssertEqual(reopened.pageSlots.count, count + 1)
    } }

    func testAMovedPageKeepsItsSizeAtItsNewPosition() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        // A blank landscape page first gives the move something to tell apart.
        let insert = try XCTUnwrap(store.beginPageEdit(.insertBlank(at: 0, orientation: .landscape)))
        store.applyPageWork(store.pageWorkResult(for: insert), for: insert)
        let last = store.pageSlots.count - 1
        let blank = store.pageSlots[0].dimensions

        let move = try XCTUnwrap(store.beginPageEdit(.move(from: 0, to: last)))
        store.applyPageWork(store.pageWorkResult(for: move), for: move)

        XCTAssertEqual(store.editStatus, "Page moved. Changes are pending save.")
        XCTAssertEqual(store.pageSlots[last].dimensions, blank)
        XCTAssertNotEqual(store.pageSlots[0].dimensions, blank)
    } }

    func testImportingAPdfAppendsAllItsPagesAsOneStep() throws { try offMain {
        let client = UniFfiPdfCoreClient()
        let store = ViewerStore(client: client)
        store.open(bytes: try Self.sampleBytes())
        let count = store.pageSlots.count
        XCTAssertNil(store.importRefusal())

        let source = try store.prepareImport(bytes: try Self.sampleBytes(), password: nil).get()
        XCTAssertEqual(source.pageCount, count)
        let request = try XCTUnwrap(store.beginImport([source]))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertEqual(store.pageSlots.count, 2 * count)
        XCTAssertEqual(store.editStatus, "Imported \(count) \(count == 1 ? "page" : "pages") from 1 PDF. Changes are pending save.")
        store.undo()
        XCTAssertFalse(store.canUndo, "the import is a single history step")
    } }

    func testAnEncryptedSourceNeedsItsPassword() throws { try offMain {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "aes_128_user_and_owner", withExtension: "pdf"))
        let bytes = try Data(contentsOf: url)
        let store = ViewerStore(client: UniFfiPdfCoreClient())

        guard case let .failure(error) = store.prepareImport(bytes: bytes, password: nil) else {
            return XCTFail("an encrypted source must not prepare without its password")
        }
        XCTAssertTrue([ViewerFailure.passwordRequired, .wrongPassword].contains(error as? ViewerFailure))
        if case let .failure(error) = store.prepareImport(bytes: bytes, password: "user-aes-pass") {
            // The core may still refuse it for its own permissions; that is
            // a refusal, not a password question.
            XCTAssertNotNil(error as? EditFailure)
        }
    } }
}
