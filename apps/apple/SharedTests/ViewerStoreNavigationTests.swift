import XCTest
@testable import Vitela

final class ViewerStoreNavigationTests: XCTestCase {
    func openStore(pageCount: Int = 5) -> ViewerStore {
        let client = FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: pageCount))
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return store
    }

    func testNoDocumentHasNoPositionAndNoNavigation() {
        let store = ViewerStore(client: FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)]))

        XCTAssertNil(store.pagePositionText)
        XCTAssertFalse(store.canGoToPreviousPage)
        XCTAssertFalse(store.canGoToNextPage)
        store.goToLastPage()
        XCTAssertNil(store.pageNavigation)
    }

    func testOpeningStartsOnTheFirstPage() {
        let store = openStore()

        XCTAssertEqual(store.currentPage, 0)
        XCTAssertEqual(store.pagePositionText, "Page 1 of 5")
        XCTAssertFalse(store.canGoToPreviousPage)
        XCTAssertTrue(store.canGoToNextPage)
    }

    func testCurrentPageIsTheFirstPageStillReachingIntoTheViewport() {
        let store = openStore()

        store.updateCurrentPage(spans: [
            0: PageViewportSpan(top: -1600, bottom: -808),
            1: PageViewportSpan(top: -792, bottom: -16),
            2: PageViewportSpan(top: -400, bottom: 392),
            3: PageViewportSpan(top: 408, bottom: 1200),
        ])

        XCTAssertEqual(store.currentPage, 2)
        XCTAssertEqual(store.pagePositionText, "Page 3 of 5")
    }

    /// A page scrolled exactly to the top leaves the previous page's bottom
    /// edge at or above the viewport top — that page is no longer current.
    func testAPageEndingExactlyAtTheTopIsNotCurrent() {
        let store = openStore()

        store.updateCurrentPage(spans: [
            0: PageViewportSpan(top: -792, bottom: 0),
            1: PageViewportSpan(top: 16, bottom: 808),
        ])

        XCTAssertEqual(store.currentPage, 1)
    }

    func testSpansForPagesTheDocumentNoLongerHasAreIgnored() {
        let store = openStore(pageCount: 2)

        store.updateCurrentPage(spans: [7: PageViewportSpan(top: 0, bottom: 100)])

        XCTAssertEqual(store.currentPage, 0)
    }

    func testStepMovesFromTheCurrentPageAndClampsAtTheEnds() {
        let store = openStore()

        store.stepPage(by: 1)
        XCTAssertEqual(store.pageNavigation?.page, 1)
        store.stepPage(by: 1)
        XCTAssertEqual(store.currentPage, 2, "a second step before the scroll reports back still moves from the requested page")

        store.goToLastPage()
        store.stepPage(by: 1)
        XCTAssertEqual(store.currentPage, 4)
        XCTAssertFalse(store.canGoToNextPage)

        store.goToFirstPage()
        store.stepPage(by: -1)
        XCTAssertEqual(store.currentPage, 0)
        XCTAssertEqual(store.pageNavigation?.page, 0)
    }

    func testRepeatingARequestForTheSamePageIsStillObservable() throws {
        let store = openStore()

        store.goToPage(3)
        let first = try XCTUnwrap(store.pageNavigation)
        store.goToPage(3)
        let second = try XCTUnwrap(store.pageNavigation)

        XCTAssertEqual(first.page, second.page)
        XCTAssertNotEqual(first, second)
    }

    func testOutOfRangePageIsRefused() {
        let store = openStore()

        store.goToPage(5)
        store.goToPage(-1)

        XCTAssertNil(store.pageNavigation)
        XCTAssertEqual(store.currentPage, 0)
    }

    func testTypedPageNumbersMustBeWholeNumbersWithinTheDocument() {
        let store = openStore()

        XCTAssertEqual(store.pageIndex(forTypedNumber: "1"), 0)
        XCTAssertEqual(store.pageIndex(forTypedNumber: " 5 "), 4)
        XCTAssertNil(store.pageIndex(forTypedNumber: "0"))
        XCTAssertNil(store.pageIndex(forTypedNumber: "6"))
        XCTAssertNil(store.pageIndex(forTypedNumber: "2.5"))
        XCTAssertNil(store.pageIndex(forTypedNumber: ""))
        XCTAssertNil(store.pageIndex(forTypedNumber: "two"))
    }

    func testOpeningAnotherDocumentResetsThePosition() {
        let store = openStore()
        store.goToPage(3)

        store.open(bytes: Data([2]))

        XCTAssertEqual(store.currentPage, 0)
        XCTAssertNil(store.pageNavigation)
    }
}

extension ViewerStoreNavigationTests {
    private static func spans(pageTops: [Int: Double], height: Double = 792) -> [Int: PageViewportSpan] {
        pageTops.mapValues { PageViewportSpan(top: $0, bottom: $0 + height) }
    }

    /// The last page cannot reach the viewport top: the scroll stops at the
    /// end with an earlier page still showing above it. The readout must
    /// keep the page that was asked for.
    func testARequestedPageThatCannotReachTheTopStaysCurrentWhereTheScrollStopped() throws {
        let store = openStore()
        store.goToLastPage()
        let request = try XCTUnwrap(store.pageNavigation)

        store.updateCurrentPage(spans: Self.spans(pageTops: [3: -500, 4: 308]))
        store.pageNavigationLanded(request)
        store.updateCurrentPage(spans: Self.spans(pageTops: [3: -500, 4: 308]))

        XCTAssertEqual(store.currentPage, 4)
        XCTAssertFalse(store.canGoToNextPage)
    }

    func testScrollingAwayFromALandedRequestFallsBackToTheFirstVisiblePage() throws {
        let store = openStore()
        store.goToLastPage()
        let request = try XCTUnwrap(store.pageNavigation)
        store.updateCurrentPage(spans: Self.spans(pageTops: [3: -500, 4: 308]))
        store.pageNavigationLanded(request)

        store.updateCurrentPage(spans: Self.spans(pageTops: [2: -900, 3: -100, 4: 708]))

        XCTAssertEqual(store.currentPage, 3)
        XCTAssertNil(store.requestedPage)
    }

    /// Spans reported while the scroll is still on its way (the requested
    /// page not laid out yet) must not snap the readout back to where the
    /// reader started.
    func testSpansBeforeTheRequestedPageIsLaidOutKeepTheRequest() {
        let store = openStore()
        store.goToPage(4)

        store.updateCurrentPage(spans: Self.spans(pageTops: [0: 16, 1: 824]))

        XCTAssertEqual(store.currentPage, 4)
    }

    /// Positions reported before the scroll view confirms the request are the
    /// scroll settling, not the reader: the resting place is the last one.
    func testMovementBeforeLandingUpdatesTheRestingPlace() throws {
        let store = openStore()
        store.goToPage(4)
        let request = try XCTUnwrap(store.pageNavigation)

        store.updateCurrentPage(spans: Self.spans(pageTops: [4: 900]))
        store.updateCurrentPage(spans: Self.spans(pageTops: [3: -500, 4: 308]))
        store.pageNavigationLanded(request)
        store.updateCurrentPage(spans: Self.spans(pageTops: [3: -500, 4: 308.4]))

        XCTAssertEqual(store.currentPage, 4)
    }

    func testAStaleLandingDoesNotConfirmANewerRequest() throws {
        let store = openStore()
        store.goToPage(4)
        let stale = try XCTUnwrap(store.pageNavigation)
        store.goToPage(2)

        store.pageNavigationLanded(stale)

        XCTAssertEqual(store.requestedPage?.landed, false)
    }
}
