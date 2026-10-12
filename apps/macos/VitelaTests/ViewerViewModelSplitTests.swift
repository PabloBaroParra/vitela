import XCTest
@testable import Vitela

final class ViewerViewModelSplitTests: XCTestCase {
    private var folder: URL!

    override func setUpWithError() throws {
        folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("vitela-split-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: folder)
    }

    private func makeModel(_ client: FakePdfCoreClient, folder: URL?, replace: Bool = false, asked: @escaping (Int, Int) -> Void = { _, _ in }) -> ViewerViewModel {
        let prompts = SavePrompts(
            chooseDestination: { _ in nil },
            confirmSignatureLoss: { false },
            resolveUnsavedChanges: { .cancel },
            chooseFolder: { folder },
            confirmReplace: { existing, total in asked(existing, total); return replace }
        )
        let model = ViewerViewModel(store: ViewerStore(client: client), prompts: prompts, sampleLoader: { Data([1]) })
        model.openSample()
        waitUntil({ model.store.state == .loaded }, "the sample never opened")
        return model
    }

    private func client(_ count: Int = 4) -> FakePdfCoreClient {
        FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: count))
    }

    func testEveryPartIsWrittenIntoTheChosenFolder() throws {
        let client = client()
        let model = makeModel(client, folder: folder)
        model.requestSplitPages()
        XCTAssertTrue(model.isSplitPresented)

        XCTAssertNil(model.splitPages(cuts: "1,3"))

        waitUntil({ !model.isSaving && model.store.editStatus.hasPrefix("Split into") }, "the split never finished")
        let names = try FileManager.default.contentsOfDirectory(atPath: folder.path).sorted()
        XCTAssertEqual(names, ["vitela-sample-1.pdf", "vitela-sample-2.pdf", "vitela-sample-3.pdf"])
        XCTAssertEqual(client.extractedPages, [[0], [1, 2], [3]])
        XCTAssertEqual(model.store.editStatus, "Split into 3 PDFs in \(folder.path).")
    }

    func testABadCutKeepsTheSheetOpen() {
        let model = makeModel(client(), folder: folder)
        model.requestSplitPages()

        XCTAssertEqual(model.splitPages(cuts: "4"), "\"4\" is not a page to split after.")
        XCTAssertTrue(model.isSplitPresented)
    }

    func testExistingFilesAreAskedAboutOnceAndDecliningWritesNothing() throws {
        try Data("old".utf8).write(to: folder.appendingPathComponent("vitela-sample-2.pdf"))
        let client = client()
        var asked: [(Int, Int)] = []
        let model = makeModel(client, folder: folder, replace: false) { asked.append(($0, $1)) }
        model.requestSplitPages()

        XCTAssertNil(model.splitPages(cuts: "2"))

        waitUntil({ model.store.editStatus == "Split cancelled." }, "the decline was never reported")
        XCTAssertEqual(asked.map(\.0), [1])
        XCTAssertEqual(asked.map(\.1), [2])
        XCTAssertTrue(client.extractedPages.isEmpty)
        XCTAssertEqual(try String(contentsOf: folder.appendingPathComponent("vitela-sample-2.pdf")), "old")
    }

    func testAFailingPartStopsTheSplitAndSaysHowFarItGot() {
        let client = client()
        client.extractError = EditFailure.failed("Disk full.")
        client.failExtractsAfter = 1
        let model = makeModel(client, folder: folder)
        model.requestSplitPages()

        XCTAssertNil(model.splitPages(cuts: "1,2"))

        waitUntil({ !model.isSaving && model.store.editStatus.hasPrefix("Split stopped") }, "the failure was never reported")
        XCTAssertEqual(model.store.editStatus, "Split stopped after 1 written: vitela-sample-2.pdf could not be written: Disk full.")
        XCTAssertEqual(client.extractedPages.count, 2, "nothing after the failing part is attempted")
    }

    func testCancellingTheFolderWritesNothing() {
        let client = client()
        let model = makeModel(client, folder: nil)
        model.requestSplitPages()

        XCTAssertNil(model.splitPages(cuts: "1"))

        waitUntil({ model.store.editStatus == "Split cancelled. No file was written." }, "the cancel was never reported")
        XCTAssertTrue(client.extractedPages.isEmpty)
    }
}
