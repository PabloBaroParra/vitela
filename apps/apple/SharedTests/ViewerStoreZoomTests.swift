import XCTest
@testable import Vitela

final class ViewerStoreZoomTests: XCTestCase {
    private let portrait = PageDimensions(width: 600, height: 800)
    private let landscape = PageDimensions(width: 800, height: 600)

    private func openStore(_ pages: [PageDimensions]) -> ViewerStore {
        let store = ViewerStore(client: FakePdfCoreClient(pages: pages))
        store.open(bytes: Data([1]))
        return store
    }

    func testCustomZoomAppliesTheSameFactorToEveryPage() {
        let store = openStore([portrait, landscape])
        store.setViewport(ViewportSize(width: 900, height: 700))

        store.setZoom(1.5)

        XCTAssertEqual(store.zoomMode, .custom)
        XCTAssertEqual(store.zoom(forPage: 0), 1.5)
        XCTAssertEqual(store.zoom(forPage: 1), 1.5)
    }

    func testFitWidthResolvesEachPageAgainstTheViewportWidth() {
        let store = openStore([portrait, landscape])
        store.setViewport(ViewportSize(width: 1200, height: 700))

        store.setZoomMode(.fitWidth)

        XCTAssertEqual(store.zoom(forPage: 0), 2.0)
        XCTAssertEqual(store.zoom(forPage: 1), 1.5)
    }

    func testFitPageUsesWhicheverSideIsTighter() {
        let store = openStore([portrait, landscape])
        store.setViewport(ViewportSize(width: 1200, height: 1200))

        store.setZoomMode(.fitPage)

        XCTAssertEqual(store.zoom(forPage: 0), 1.5, "the portrait page is bound by its height")
        XCTAssertEqual(store.zoom(forPage: 1), 1.5, "the landscape page is bound by its width")
    }

    func testFitFactorsStayWithinTheZoomBounds() {
        let store = openStore([portrait])
        store.setZoomMode(.fitWidth)

        store.setViewport(ViewportSize(width: 60, height: 80))
        XCTAssertEqual(store.zoom(forPage: 0), ViewerStore.minimumZoom)

        store.setViewport(ViewportSize(width: 60000, height: 80000))
        XCTAssertEqual(store.zoom(forPage: 0), ViewerStore.maximumZoom)
    }

    /// The first layout pass may run before the scroll view has a size; a
    /// fit mode must not collapse the page to nothing in that case.
    func testFitModeWithoutAMeasuredViewportFallsBackToTheExplicitFactor() {
        let store = openStore([portrait])
        store.setZoomMode(.fitWidth)

        XCTAssertEqual(store.zoom(forPage: 0), 1.0)
        store.setViewport(ViewportSize(width: 0, height: 0))
        XCTAssertEqual(store.zoom(forPage: 0), 1.0)
    }

    func testAViewportChangeMakesAFittedPageStaleButNotAnExplicitOne() throws {
        let client = FakePdfCoreClient(pages: [portrait])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        store.setViewport(ViewportSize(width: 600, height: 800))
        store.setZoomMode(.fitWidth)
        let fitted = try XCTUnwrap(store.beginRender(page: 0))
        store.applyRender(store.renderResult(for: fitted), for: fitted)
        XCTAssertEqual(fitted.dpi, 72)

        store.setViewport(ViewportSize(width: 1200, height: 800))
        let refitted = try XCTUnwrap(store.beginRender(page: 0), "a wider viewport must re-render the fitted page")
        XCTAssertEqual(refitted.dpi, 144)
        store.applyRender(store.renderResult(for: refitted), for: refitted)

        store.setZoom(2)
        store.setViewport(ViewportSize(width: 600, height: 800))
        XCTAssertNil(store.beginRender(page: 0), "an explicit zoom must not follow the viewport")
    }

    func testZoomStepsFollowTheLadderFromTheFactorShown() {
        let store = openStore([portrait])
        store.setViewport(ViewportSize(width: 660, height: 800))
        store.setZoomMode(.fitWidth)
        XCTAssertEqual(store.displayedZoom, 1.1, accuracy: 1e-9)

        store.stepZoom(1)
        XCTAssertEqual(store.zoomMode, .custom, "stepping leaves the fit mode")
        XCTAssertEqual(store.zoom, 1.25)

        store.stepZoom(-1)
        store.stepZoom(-1)
        XCTAssertEqual(store.zoom, 0.75)
    }

    func testZoomStepsStopAtTheBounds() {
        let store = openStore([portrait])

        store.setZoom(ViewerStore.maximumZoom)
        store.stepZoom(1)
        XCTAssertEqual(store.zoom, ViewerStore.maximumZoom)

        store.setZoom(ViewerStore.minimumZoom)
        store.stepZoom(-1)
        XCTAssertEqual(store.zoom, ViewerStore.minimumZoom)
    }

    func testLeavingAFitModeKeepsTheFactorShown() {
        let store = openStore([portrait])
        store.setViewport(ViewportSize(width: 900, height: 800))
        store.setZoomMode(.fitWidth)

        store.setZoomMode(.custom)

        XCTAssertEqual(store.zoomMode, .custom)
        XCTAssertEqual(store.zoom, 1.5)
    }
}

extension ViewerStoreZoomTests {
    func testTheReadoutFollowsTheCurrentPage() {
        let store = ViewerStore(client: FakePdfCoreClient(pages: [
            PageDimensions(width: 600, height: 800), PageDimensions(width: 800, height: 600),
        ]))
        store.open(bytes: Data([1]))
        store.setViewport(ViewportSize(width: 1200, height: 700))
        store.setZoomMode(.fitWidth)
        XCTAssertEqual(store.displayedZoom, 2.0)

        store.goToPage(1)

        XCTAssertEqual(store.displayedZoom, 1.5, "a landscape page in fit width shows its own factor")
    }
}
