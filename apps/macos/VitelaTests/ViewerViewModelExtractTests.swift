import XCTest
@testable import Vitela

final class ViewerViewModelExtractTests: XCTestCase {
    private var directory: URL!

    override func setUpWithError() throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("vitela-extract-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: directory)
    }

    private func makeModel(_ client: FakePdfCoreClient, destination: URL?, asked: @escaping (String) -> Void = { _ in }) -> ViewerViewModel {
        let prompts = SavePrompts(
            chooseDestination: { name in asked(name); return destination },
            confirmSignatureLoss: { false },
            resolveUnsavedChanges: { .cancel }
        )
        let model = ViewerViewModel(store: ViewerStore(client: client), prompts: prompts)
        model.store.open(bytes: Data([1]))
        return model
    }

    private func client() -> FakePdfCoreClient {
        FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: 3))
    }

    func testARefusingDocumentNeverShowsTheRangeSheet() {
        let client = client()
        client.textExtractionPermitted = false
        let model = makeModel(client, destination: nil)

        model.requestExtractPages()

        XCTAssertFalse(model.isExtractPresented)
        XCTAssertEqual(model.store.editStatus, "This document does not permit extracting its pages.")
    }

    func testABadRangeKeepsTheSheetOpenWithTheReason() {
        let model = makeModel(client(), destination: nil)
        model.requestExtractPages()

        let refusal = model.extractPages(range: "7")

        XCTAssertEqual(refusal, "\"7\" is not a page range in this document.")
        XCTAssertTrue(model.isExtractPresented)
    }

    func testTheChosenPagesAreWrittenToTheChosenFile() throws {
        let client = client()
        client.sourceIsSigned = true
        let destination = directory.appendingPathComponent("out.pdf")
        var suggested: [String] = []
        let model = makeModel(client, destination: destination) { suggested.append($0) }
        model.requestExtractPages()

        XCTAssertNil(model.extractPages(range: "1-2"))
        XCTAssertFalse(model.isExtractPresented)

        waitUntil({ FileManager.default.fileExists(atPath: destination.path) && !model.isSaving }, "the extract was never written")
        XCTAssertEqual(try Data(contentsOf: destination), client.extractedBytes)
        XCTAssertEqual(suggested, ["document.pdf"])
        XCTAssertEqual(client.extractedPages, [[0, 1]])
        XCTAssertEqual(
            model.store.editStatus,
            "Extracted 2 pages to \(destination.path). The original document is signed, so the extracted PDF's signature no longer verifies."
        )
    }

    func testCancellingThePanelWritesNothing() {
        let client = client()
        let model = makeModel(client, destination: nil)
        model.requestExtractPages()

        XCTAssertNil(model.extractPages(range: "1"))

        waitUntil({ model.store.editStatus == "Extract cancelled. No file was written." }, "the cancel was never reported")
        XCTAssertTrue(client.extractedPages.isEmpty)
    }

    func testAFailedExtractReportsTheCoreReason() {
        let client = client()
        client.extractError = EditFailure.failed("Could not prune the document.")
        let model = makeModel(client, destination: directory.appendingPathComponent("out.pdf"))
        model.requestExtractPages()

        XCTAssertNil(model.extractPages(range: "1"))

        waitUntil({ model.store.editStatus == "Could not prune the document." }, "the failure was never reported")
        XCTAssertFalse(model.isSaving)
        XCTAssertFalse(FileManager.default.fileExists(atPath: self.directory.appendingPathComponent("out.pdf").path))
    }
}
