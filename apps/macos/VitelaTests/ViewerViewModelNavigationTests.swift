import XCTest
@testable import Vitela

final class ViewerViewModelNavigationTests: XCTestCase {
    private func makeModel(pageCount: Int) -> ViewerViewModel {
        let pages = Array(repeating: PageDimensions(width: 612, height: 792), count: pageCount)
        return ViewerViewModel(store: ViewerStore(client: FakePdfCoreClient(pages: pages)))
    }

    func testGoToPagePromptNeedsAnOpenDocument() {
        let model = makeModel(pageCount: 3)

        model.requestGoToPage()

        XCTAssertFalse(model.isGoToPagePresented)
    }

    func testValidPageNumberClosesThePromptAndRequestsTheScroll() {
        let model = makeModel(pageCount: 3)
        model.store.open(bytes: Data([1]))
        model.requestGoToPage()
        XCTAssertTrue(model.isGoToPagePresented)

        XCTAssertTrue(model.goToTypedPage("3"))

        XCTAssertFalse(model.isGoToPagePresented)
        XCTAssertEqual(model.store.pageNavigation?.page, 2)
        XCTAssertEqual(model.store.pagePositionText, "Page 3 of 3")
    }

    func testInvalidPageNumberKeepsThePromptOpenAndDoesNotScroll() {
        let model = makeModel(pageCount: 3)
        model.store.open(bytes: Data([1]))
        model.requestGoToPage()

        XCTAssertFalse(model.goToTypedPage("4"))

        XCTAssertTrue(model.isGoToPagePresented)
        XCTAssertNil(model.store.pageNavigation)
    }
}
