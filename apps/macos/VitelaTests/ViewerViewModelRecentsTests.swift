import XCTest
@testable import Vitela

final class ViewerViewModelRecentsTests: XCTestCase {
    private var folder: URL!

    override func setUpWithError() throws {
        folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("vitela-recents-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: folder)
    }

    private func pdf(_ name: String) throws -> URL {
        let url = folder.appendingPathComponent(name)
        try Data("%PDF-1.7".utf8).write(to: url)
        return url.standardizedFileURL
    }

    private func makeModel(_ client: FakePdfCoreClient = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)]),
                           store: RecentDocumentStore = InMemoryRecentDocumentStore()) -> ViewerViewModel {
        ViewerViewModel(store: ViewerStore(client: client), recentStore: store, sampleLoader: { Data([1]) })
    }

    func testAFileThatOpensIsRecordedAndSaved() throws {
        let file = try pdf("report.pdf")
        let recents = InMemoryRecentDocumentStore()
        let model = makeModel(store: recents)

        model.open(url: file)

        waitUntil({ model.recentDocuments.first?.path == file.path }, "the open was never recorded")
        waitUntil({ recents.load().map(\.path) == [file.path] }, "the list was never saved")
    }

    func testTheSampleAndNewDocumentsAreNotRecorded() {
        let model = makeModel()

        model.openSample()
        waitUntil({ model.store.state == .loaded }, "the sample never opened")
        model.newDocument()

        XCTAssertNil(model.pendingRecentPath)
        XCTAssertTrue(model.recentDocuments.isEmpty)
    }

    func testAFileThatFailsToOpenIsNotRecorded() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.openError = ViewerFailure.openFailed("not a PDF")
        let model = makeModel(client)

        model.open(url: try pdf("broken.pdf"))

        waitUntil({ if case .error = model.store.state { return true } else { return false } }, "the open never failed")
        XCTAssertNil(model.pendingRecentPath)
        XCTAssertTrue(model.recentDocuments.isEmpty)
    }

    func testAPasswordPromptKeepsTheFilePending() throws {
        let locked = try pdf("locked.pdf")
        let model = makeModel()
        model.pendingRecentPath = locked.path

        model.recordPendingRecent(.error(.passwordRequired))
        XCTAssertEqual(model.pendingRecentPath, locked.path)

        model.recordPendingRecent(.loaded)
        XCTAssertNil(model.pendingRecentPath)
        waitUntil({ model.recentDocuments.map(\.path) == [locked.path] }, "the unlocked file was never recorded")
    }

    func testTheStoredListLoadsWithoutFilesThatAreGone() throws {
        let kept = try pdf("kept.pdf")
        let recents = InMemoryRecentDocumentStore([
            RecentDocument(path: kept.path, openedAt: Date()),
            RecentDocument(path: folder.appendingPathComponent("gone.pdf").path, openedAt: Date()),
        ])

        let model = makeModel(store: recents)

        waitUntil({ !model.recentDocuments.isEmpty }, "the list never loaded")
        XCTAssertEqual(model.recentDocuments.map(\.path), [kept.path])
    }

    func testRemovingAndClearingChangeTheStoredList() throws {
        let a = try pdf("a.pdf"), b = try pdf("b.pdf")
        let recents = InMemoryRecentDocumentStore([
            RecentDocument(path: a.path, openedAt: Date()),
            RecentDocument(path: b.path, openedAt: Date()),
        ])
        let model = makeModel(store: recents)
        waitUntil({ model.recentDocuments.count == 2 }, "the list never loaded")

        model.removeRecent(model.recentDocuments[0])
        waitUntil({ recents.load().map(\.path) == [b.path] }, "the removal was never saved")
        XCTAssertEqual(model.recentDocuments.map(\.path), [b.path])

        model.clearRecents()
        waitUntil({ recents.load().isEmpty }, "the clear was never saved")
        XCTAssertTrue(model.recentDocuments.isEmpty)
    }

    func testARecordMadeBeforeTheListLoadsKeepsTheOlderEntries() throws {
        let old = try pdf("old.pdf"), new = try pdf("new.pdf")
        let recents = InMemoryRecentDocumentStore([RecentDocument(path: old.path, openedAt: Date(timeIntervalSinceNow: -60))])
        let model = makeModel(store: recents)

        model.recordRecent(new.path)

        waitUntil({ recents.load().count == 2 }, "the record lost the stored list")
        waitUntil({ model.recentDocuments.map(\.path) == [new.path, old.path] }, "the list never showed both")
    }

    func testPreviewsRenderPageOneOnceAndCountThePages() throws {
        let file = try pdf("three.pdf")
        let client = FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: 3))
        let model = makeModel(client, store: InMemoryRecentDocumentStore([RecentDocument(path: file.path, openedAt: Date())]))
        waitUntil({ !model.recentDocuments.isEmpty }, "the list never loaded")

        model.loadRecentPreviews(scale: 2)
        model.loadRecentPreviews(scale: 2)

        waitUntil({ model.recentPreviews[file.path] != nil }, "the preview never arrived")
        XCTAssertEqual(model.recentPreviews[file.path]?.pageCount, 3)
        XCTAssertEqual(client.requestedPages, [0])
        XCTAssertEqual(client.requestedDpi, [48])
    }

    func testOpeningACardOpensItsFile() throws {
        let file = try pdf("card.pdf")
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = makeModel(client)

        model.openRecent(RecentDocument(path: file.path, openedAt: Date()))

        waitUntil({ model.store.state == .loaded }, "the card never opened")
        XCTAssertEqual(model.documentName, "card.pdf")
    }
}
