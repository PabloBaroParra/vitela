import XCTest
@testable import Vitela

final class ViewerViewModelPagesTests: XCTestCase {
    private let letter = PageDimensions(width: 612, height: 792)
    private let a5 = PageDimensions(width: 420, height: 595)

    private func makeModel(_ client: FakePdfCoreClient) -> ViewerViewModel {
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))
        return model
    }

    func testAPageEditRunsOffTheMainThreadAndRelaysTheReaderOut() {
        let client = FakePdfCoreClient(pages: [letter, a5])
        let model = makeModel(client)

        model.editPages(.rotate(page: 0, degrees: 90))

        XCTAssertTrue(model.store.pageWorkInFlight)
        waitUntil({ !model.store.pageWorkInFlight }, "the page edit never finished")
        XCTAssertEqual(model.store.pageSlots.map(\.dimensions), [PageDimensions(width: 792, height: 612), a5])
        XCTAssertEqual(model.store.editStatus, "Page rotated. Changes are pending save.")
    }

    func testUndoingAPageEditRestoresTheLayout() {
        let client = FakePdfCoreClient(pages: [letter, a5])
        let model = makeModel(client)
        model.editPages(.rotate(page: 1, degrees: -90))
        waitUntil({ !model.store.pageWorkInFlight }, "the page edit never finished")

        model.undo()

        waitUntil({ model.store.pageSlots.map(\.dimensions) == [self.letter, self.a5] }, "undo never relaid the pages out")
        XCTAssertEqual(client.refreshPreviewCount, 2)

        model.redo()
        waitUntil({ model.store.pageSlots[1].dimensions == PageDimensions(width: 595, height: 420) }, "redo never relaid the pages out")
    }

    func testUndoOfAMetadataOnlySessionDoesNotRebuildThePreview() {
        let client = FakePdfCoreClient(pages: [letter])
        let model = makeModel(client)
        model.store.setDocumentProperty(.title, to: "Edited")

        model.undo()

        XCTAssertFalse(model.store.pageWorkInFlight)
        XCTAssertEqual(client.refreshPreviewCount, 0)
    }

    func testThumbnailsRenderOnTheQueue() {
        let client = FakePdfCoreClient(pages: [letter])
        let model = makeModel(client)

        model.renderThumbnail(page: 0, scale: 2)

        waitUntil({ model.store.thumbnails[0] != nil }, "the thumbnail never arrived")
        XCTAssertEqual(client.requestedDpi, [28])
    }

    func testOpeningAnotherDocumentLeavesOrganize() {
        let client = FakePdfCoreClient(pages: [letter])
        let model = ViewerViewModel(store: ViewerStore(client: client), sampleLoader: { Data([2]) })
        model.store.open(bytes: Data([1]))
        model.isOrganizing = true

        model.openSample()

        waitUntil({ client.openedBytes.count == 2 }, "the sample never opened")
        XCTAssertFalse(model.isOrganizing)
    }
}

extension ViewerViewModelPagesTests {
    private func threePageModel() -> (FakePdfCoreClient, ViewerViewModel) {
        let client = FakePdfCoreClient(pages: [
            PageDimensions(width: 100, height: 200), PageDimensions(width: 300, height: 400), PageDimensions(width: 500, height: 600),
        ])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))
        return (client, model)
    }

    func testDroppingADraggedCardMovesThePageThere() {
        let (client, model) = threePageModel()

        let token = model.beginPageDrag(page: 0)
        XCTAssertTrue(model.dropPage(token: token, on: 2))

        waitUntil({ !model.store.pageWorkInFlight }, "the move never finished")
        XCTAssertEqual(client.pageEdits, [.move(from: 0, to: 2)])
        XCTAssertNil(model.organizePageDrag)
    }

    func testAForeignOrStaleDropMovesNothing() {
        let (client, model) = threePageModel()

        _ = model.beginPageDrag(page: 0)
        XCTAssertFalse(model.dropPage(token: "text from another app", on: 2))

        let token = model.beginPageDrag(page: 1)
        model.store.open(bytes: Data([2]))
        XCTAssertFalse(model.dropPage(token: token, on: 2), "a drag picked up from an older layout")

        let selfDrop = model.beginPageDrag(page: 1)
        XCTAssertFalse(model.dropPage(token: selfDrop, on: 1))
        XCTAssertTrue(client.pageEdits.isEmpty)
    }
}
