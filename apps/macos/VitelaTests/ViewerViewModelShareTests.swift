import XCTest
@testable import Vitela

final class ViewerViewModelShareTests: XCTestCase {
    private var root: URL!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("vitela-share-tests-\(UUID().uuidString)", isDirectory: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: root)
    }

    private func makeModel(
        _ client: FakePdfCoreClient,
        confirm: @escaping () -> Bool = { false },
        presented: @escaping (URL) -> Bool
    ) -> ViewerViewModel {
        let prompts = SharePrompts(confirmSignatureLoss: confirm, present: presented, root: root)
        let model = ViewerViewModel(store: ViewerStore(client: client), sharePrompts: prompts, sampleLoader: { Data([1]) })
        model.openSample()
        waitUntil({ model.store.state == .loaded }, "the sample never opened")
        return model
    }

    private func client() -> FakePdfCoreClient {
        FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
    }

    func testSharingHandsThePickerTheSavedBytesUnderTheDocumentsName() throws {
        let client = client()
        var shared: URL?
        let model = makeModel(client) { shared = $0; return true }
        XCTAssertTrue(model.canShare)

        model.share()

        waitUntil({ !model.isSaving }, "the share never finished")
        let file = try XCTUnwrap(shared)
        XCTAssertEqual(file.lastPathComponent, "vitela-sample.pdf")
        XCTAssertEqual(try Data(contentsOf: file), client.savedBytes)
        XCTAssertTrue(file.path.hasPrefix(root.path))
        XCTAssertEqual(model.store.editStatus, "")
        XCTAssertTrue(model.store.hasUnsavedChanges == false, "sharing is not a save")
    }

    func testASignedDocumentAsksFirstAndCancellingSharesNothing() {
        let client = client()
        client.invalidatesSignatures = true
        var asked = false
        var shared = false
        let model = makeModel(client, confirm: { asked = true; return false }) { _ in shared = true; return true }

        model.share()

        waitUntil({ !model.isSaving }, "the share never finished")
        XCTAssertTrue(asked)
        XCTAssertFalse(shared)
        XCTAssertEqual(model.store.editStatus, "Share cancelled.")
    }

    func testAcceptingTheSignatureLossSharesTheRewrittenCopy() {
        let client = client()
        client.invalidatesSignatures = true
        var shared = false
        let model = makeModel(client, confirm: { true }) { _ in shared = true; return true }

        model.share()

        waitUntil({ !model.isSaving }, "the share never finished")
        XCTAssertTrue(shared)
    }

    func testAFailedSaveSaysWhyAndSharesNothing() {
        let client = client()
        client.saveError = EditFailure.failed("Disk on fire.")
        var shared = false
        let model = makeModel(client) { _ in shared = true; return true }

        model.share()

        waitUntil({ !model.isSaving }, "the share never finished")
        XCTAssertFalse(shared)
        XCTAssertTrue(model.store.editStatus.hasPrefix("Could not share:"), model.store.editStatus)
    }

    func testNothingToShareWithoutADocument() {
        let model = ViewerViewModel(
            store: ViewerStore(client: client()),
            sharePrompts: SharePrompts(confirmSignatureLoss: { false }, present: { _ in XCTFail("no picker"); return true }, root: root)
        )
        XCTAssertFalse(model.canShare)

        model.share()

        XCTAssertEqual(model.store.editStatus, "Open a PDF before sharing.")
        XCTAssertFalse(model.isSaving)
    }

    func testShareFileNamesAreSafeAndEndInPdf() {
        XCTAssertEqual(ViewerViewModel.shareFileName(for: "report"), "report.pdf")
        XCTAssertEqual(ViewerViewModel.shareFileName(for: "Report.PDF"), "Report.PDF")
        XCTAssertEqual(ViewerViewModel.shareFileName(for: " a/b:c.pdf "), "a_b_c.pdf")
        XCTAssertEqual(ViewerViewModel.shareFileName(for: "  "), "document.pdf")
        XCTAssertEqual(ViewerViewModel.shareFileName(for: ".."), "document.pdf")
    }

    func testEachShareGetsItsOwnFolderAndOldOnesAreSwept() throws {
        let first = try ViewerViewModel.writeShareCopy(Data("1".utf8), named: "a.pdf", under: root)
        let second = try ViewerViewModel.writeShareCopy(Data("2".utf8), named: "a.pdf", under: root)
        XCTAssertNotEqual(first.deletingLastPathComponent(), second.deletingLastPathComponent())
        XCTAssertEqual(try String(contentsOf: first), "1", "a recent copy may still be read by its target")

        let later = Date().addingTimeInterval(ViewerViewModel.shareCopyLifetime + 60)
        let third = try ViewerViewModel.writeShareCopy(Data("3".utf8), named: "a.pdf", under: root, now: later)

        XCTAssertFalse(FileManager.default.fileExists(atPath: first.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: second.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: third.path))
    }
}
