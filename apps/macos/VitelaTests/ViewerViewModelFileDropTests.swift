import XCTest
@testable import Vitela

final class ViewerViewModelFileDropTests: XCTestCase {
    private var directory: URL!

    override func setUpWithError() throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("vitela-drop-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: directory)
    }

    private func file(_ name: String, _ contents: String) throws -> URL {
        let url = directory.appendingPathComponent(name)
        try Data(contents.utf8).write(to: url)
        return url
    }

    private func makeModel(_ client: FakePdfCoreClient, unsavedChoice: SavePrompts.UnsavedChoice = .cancel) -> ViewerViewModel {
        let prompts = SavePrompts(
            chooseDestination: { _ in nil },
            confirmSignatureLoss: { false },
            resolveUnsavedChanges: { unsavedChoice }
        )
        return ViewerViewModel(store: ViewerStore(client: client), prompts: prompts)
    }

    func testADroppedPdfOpensUnderItsOwnName() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = makeModel(client)
        let pdf = try file("report", "%PDF-1.7 body")

        model.openExternalFiles([pdf])

        waitUntil({ model.store.state == .loaded }, "the dropped PDF never opened")
        XCTAssertEqual(client.openedBytes, [Data("%PDF-1.7 body".utf8)])
        XCTAssertEqual(model.documentName, "report", "a PDF without the extension is still a PDF")
    }

    func testOnlyTheFirstFileOfADropOpens() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = makeModel(client)

        model.openExternalFiles([try file("a.pdf", "%PDF-first"), try file("b.pdf", "%PDF-second")])

        waitUntil({ model.store.state == .loaded }, "the dropped PDF never opened")
        XCTAssertEqual(client.openedBytes, [Data("%PDF-first".utf8)])
    }

    func testANonPdfIsRefusedWithANoticeAndLeavesTheOpenDocumentAlone() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = makeModel(client)
        model.store.open(bytes: Data([1]))

        model.openExternalFiles([try file("notes.pdf", "plain text with a pdf name")])

        waitUntil({ !model.notice.isEmpty }, "the refusal was never reported")
        XCTAssertEqual(model.notice, ViewerViewModel.unsupportedDropMessage)
        XCTAssertEqual(client.openedBytes.count, 1)
        XCTAssertEqual(model.store.state, .loaded)
    }

    func testAnUnreadableFileIsReported() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = makeModel(client)

        model.openExternalFiles([directory.appendingPathComponent("missing.pdf")])

        waitUntil({ !model.notice.isEmpty }, "the read failure was never reported")
        XCTAssertEqual(model.notice, "The dropped file could not be read.")
        XCTAssertTrue(client.openedBytes.isEmpty)
    }

    func testUnsavedChangesAreAskedAboutAndCancelKeepsTheDocument() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = makeModel(client, unsavedChoice: .cancel)
        model.store.open(bytes: Data([1]))
        model.store.setDocumentProperty(.title, to: "Edited")
        XCTAssertTrue(model.store.hasUnsavedChanges)

        model.openExternalFiles([try file("other.pdf", "%PDF-other")])

        let drained = expectation(description: "read finished")
        model.operationQueue.addOperation { DispatchQueue.main.async { drained.fulfill() } }
        wait(for: [drained], timeout: 2)
        XCTAssertEqual(client.openedBytes.count, 1, "Cancel must keep the edited document")
        XCTAssertTrue(model.store.hasUnsavedChanges)
    }

    func testANewOpenClearsAnEarlierNotice() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = makeModel(client)
        model.notice = ViewerViewModel.unsupportedDropMessage

        model.openExternalFiles([try file("a.pdf", "%PDF-a")])

        waitUntil({ model.store.state == .loaded }, "the dropped PDF never opened")
        XCTAssertEqual(model.notice, "")
    }
}
