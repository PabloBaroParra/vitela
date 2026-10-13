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

extension ViewerViewModelNavigationTests {
    func testThePagesColumnStartsOpenAsOnLinuxAndWindows() {
        XCTAssertTrue(makeModel(pageCount: 1).showsPages)
    }

    func testPickingAPageFromTheColumnGoesThere() {
        let model = makeModel(pageCount: 3)
        model.store.open(bytes: Data([1]))
        XCTAssertTrue(model.canPickPageFromList)

        model.pickPageFromList(2)

        XCTAssertEqual(model.store.pageNavigation?.page, 2)
        XCTAssertEqual(model.store.currentPage, 2)
    }

    func testTheColumnDoesNothingWithoutADocumentOrWhileOrganizing() {
        let model = makeModel(pageCount: 3)
        XCTAssertFalse(model.canPickPageFromList)
        model.pickPageFromList(1)
        XCTAssertNil(model.store.pageNavigation)

        model.store.open(bytes: Data([1]))
        model.isOrganizing = true
        XCTAssertFalse(model.canPickPageFromList)
        model.pickPageFromList(1)
        XCTAssertNil(model.store.pageNavigation)
    }

    func testTheColumnAsksForTheSameThumbnailsAsOrganize() {
        let client = FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: 2))
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))

        model.renderThumbnail(page: 1, scale: 2)
        waitUntil({ model.store.thumbnails[1] != nil }, "the thumbnail never arrived")
        let renders = client.requestedPages.count
        model.renderThumbnail(page: 1, scale: 2)

        XCTAssertEqual(client.requestedPages.count, renders, "a page shown in both places renders once")
    }
}
