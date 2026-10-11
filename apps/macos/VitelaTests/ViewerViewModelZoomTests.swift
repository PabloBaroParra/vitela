import XCTest
@testable import Vitela

final class ViewerViewModelZoomTests: XCTestCase {
    private func makeModel() -> ViewerViewModel {
        let store = ViewerStore(client: FakePdfCoreClient(pages: [PageDimensions(width: 600, height: 800)]))
        let model = ViewerViewModel(store: store)
        model.store.open(bytes: Data([1]))
        return model
    }

    func testDocumentsOpenFittedToTheWindowWidthLikeLinuxAndWindows() {
        XCTAssertEqual(makeModel().store.zoomMode, .fitWidth)
    }

    func testTheFirstViewportReportAppliesAtOnceAndLaterOnesSettleFirst() throws {
        let model = makeModel()
        let padding = 2 * ViewerViewModel.pageListPadding

        model.updateViewport(CGSize(width: 600 + padding, height: 800 + padding))
        let first = model.store.viewport
        XCTAssertNotNil(first, "a document must open fitted, not wait for a resize to settle")

        model.updateViewport(CGSize(width: 900 + padding, height: 800 + padding))
        model.updateViewport(CGSize(width: 1200 + padding, height: 800 + padding))
        XCTAssertEqual(model.store.viewport, first, "a resize in progress must not re-render every step")

        let settled = expectation(description: "viewport settles")
        DispatchQueue.main.asyncAfter(deadline: .now() + ViewerViewModel.viewportSettleDelay + 0.1) { settled.fulfill() }
        wait(for: [settled], timeout: 2)
        // Only the last size lands. Its width also depends on the system's
        // scroller style, which the test does not control.
        let landed = try XCTUnwrap(model.store.viewport)
        XCTAssertGreaterThan(landed.width, 1100)
        XCTAssertEqual(landed.height, 800)
    }

    func testTheViewportLeavesThePageListPadding() {
        let viewport = ViewerViewModel.pageViewport(for: CGSize(width: 500, height: 400))

        XCTAssertLessThanOrEqual(viewport.width, 500 - 2 * ViewerViewModel.pageListPadding)
        XCTAssertEqual(viewport.height, 400 - 2 * ViewerViewModel.pageListPadding)
    }
}
