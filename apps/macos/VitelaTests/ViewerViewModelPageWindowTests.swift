import XCTest
@testable import Vitela

/// A page that leaves the view before its render starts is skipped, and
/// rendered again when it comes back — Windows' `PageWindow`.
final class ViewerViewModelPageWindowTests: XCTestCase {
    private func openedModel() -> (FakePdfCoreClient, ViewerViewModel) {
        let client = FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: 3))
        let model = ViewerViewModel(store: ViewerStore(client: client), sampleLoader: { Data([1]) })
        model.openSample()
        waitUntil({ model.store.state == .loaded }, "the sample never opened")
        return (client, model)
    }

    /// Holds the serial queue until the returned closure is called.
    private func holdQueue(_ model: ViewerViewModel) -> () -> Void {
        let gate = DispatchSemaphore(value: 0)
        model.operationQueue.addOperation { gate.wait() }
        return { gate.signal() }
    }

    func testARenderStillQueuedWhenItsPageLeavesIsSkippedAndAskedForAgainOnReturn() {
        let (client, model) = openedModel()
        let release = holdQueue(model)
        model.render(page: 0)
        model.render(page: 1)
        model.pageLeftView(0, generation: model.store.generation)
        release()
        waitUntil({ model.store.pageSlots[1].status == .rendered }, "page 2 never rendered")
        model.operationQueue.waitUntilAllOperationsAreFinished()
        XCTAssertEqual(client.requestedPages, [1], "the page that left must not be rendered")

        model.render(page: 0)
        waitUntil({ model.store.pageSlots[0].status == .rendered }, "page 1 never rendered on its return")
        XCTAssertEqual(client.requestedPages, [1, 0])
    }

    func testARowOfAnEarlierLayoutLeavingDropsNothing() {
        let (client, model) = openedModel()
        let release = holdQueue(model)
        let earlier = model.store.generation
        model.store.bumpGeneration()
        model.render(page: 0)
        model.pageLeftView(0, generation: earlier)
        release()
        waitUntil({ model.store.pageSlots[0].status == .rendered }, "page 1 never rendered")
        XCTAssertEqual(client.requestedPages, [0])
    }

    func testARenderAlreadyDoneIsNotAskedForAgain() {
        let (client, model) = openedModel()
        model.render(page: 0)
        waitUntil({ model.store.pageSlots[0].status == .rendered }, "page 1 never rendered")
        model.pageLeftView(0, generation: model.store.generation)
        model.render(page: 0)
        model.operationQueue.waitUntilAllOperationsAreFinished()
        XCTAssertEqual(client.requestedPages, [0])
    }
}
