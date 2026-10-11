import XCTest
@testable import Vitela

final class ViewerViewModelImageExportTests: XCTestCase {
    private var folder: URL!

    override func setUpWithError() throws {
        folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("vitela-export-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: folder)
    }

    private func makeModel(_ client: FakePdfCoreClient, folder: URL?) -> ViewerViewModel {
        let prompts = SavePrompts(
            chooseDestination: { _ in nil },
            confirmSignatureLoss: { false },
            resolveUnsavedChanges: { .cancel },
            chooseFolder: { folder }
        )
        let model = ViewerViewModel(store: ViewerStore(client: client), prompts: prompts, sampleLoader: { Data([1]) })
        model.openSample()
        waitUntil({ model.store.state == .loaded }, "the sample never opened")
        return model
    }

    private func client() -> FakePdfCoreClient {
        FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: 3))
    }

    func testEveryChosenPageIsWrittenWithoutOverwritingEarlierExports() throws {
        try Data("earlier".utf8).write(to: folder.appendingPathComponent("vitela-sample-1.png"))
        let client = client()
        let model = makeModel(client, folder: folder)
        model.requestExportImages()

        XCTAssertNil(model.exportImages(pages: .range("1-2"), dpi: 200, format: .png))
        XCTAssertFalse(model.isExportImagesPresented)

        waitUntil({ !model.isSaving && model.store.editStatus.hasPrefix("Exported") }, "the export never finished")
        let names = try FileManager.default.contentsOfDirectory(atPath: folder.path).sorted()
        XCTAssertEqual(names, ["vitela-sample-1 (2).png", "vitela-sample-1.png", "vitela-sample-2.png"])
        XCTAssertEqual(try String(contentsOf: folder.appendingPathComponent("vitela-sample-1.png")), "earlier")
        XCTAssertEqual(client.exportedImages.map(\.dpi), [200, 200])
        XCTAssertEqual(model.store.editStatus, "Exported 2 pages as PNG to \(folder.path).")
    }

    func testARefusedChoiceKeepsTheSheetOpen() {
        let model = makeModel(client(), folder: folder)
        model.requestExportImages()

        XCTAssertEqual(model.exportImages(pages: .all, dpi: 1000, format: .png), "Choose a resolution between 72 and 400 DPI.")
        XCTAssertTrue(model.isExportImagesPresented)
    }

    func testAFailingPageStopsTheExportAndSaysHowManyWereWritten() {
        let client = client()
        client.exportError = EditFailure.failed("Render failed.")
        client.failExportsAfter = 1
        let model = makeModel(client, folder: folder)
        model.requestExportImages()

        XCTAssertNil(model.exportImages(pages: .all, dpi: 150, format: .jpeg))

        waitUntil({ !model.isSaving && model.store.editStatus.hasPrefix("Page") }, "the failure was never reported")
        XCTAssertEqual(model.store.editStatus, "Page 2 could not be exported: Render failed. 1 written.")
        XCTAssertEqual(client.exportedImages.count, 2)
    }

    func testCancellingTheFolderWritesNothing() {
        let client = client()
        let model = makeModel(client, folder: nil)
        model.requestExportImages()

        XCTAssertNil(model.exportImages(pages: .all, dpi: 150, format: .png))

        waitUntil({ model.store.editStatus == "Export cancelled. No file was written." }, "the cancel was never reported")
        XCTAssertTrue(client.exportedImages.isEmpty)
    }

    func testUniqueNamesCountUpLikeWindows() throws {
        try Data().write(to: folder.appendingPathComponent("a.png"))
        try Data().write(to: folder.appendingPathComponent("a (2).png"))

        XCTAssertEqual(ViewerViewModel.uniqueDestination(for: "a.png", in: folder).lastPathComponent, "a (3).png")
        XCTAssertEqual(ViewerViewModel.uniqueDestination(for: "b.png", in: folder).lastPathComponent, "b.png")
    }
}
