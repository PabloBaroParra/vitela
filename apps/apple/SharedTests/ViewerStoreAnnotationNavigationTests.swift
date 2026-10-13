import XCTest
@testable import Vitela

final class ViewerStoreAnnotationNavigationTests: XCTestCase {
    func testStepIdWrapsAndStartsAtEitherEnd() {
        let ids: [UInt64] = [4, 7, 9]
        XCTAssertEqual(ViewerStore.steppedAnnotationId(ids, from: nil, forward: true), 4)
        XCTAssertEqual(ViewerStore.steppedAnnotationId(ids, from: nil, forward: false), 9)
        XCTAssertEqual(ViewerStore.steppedAnnotationId(ids, from: 7, forward: true), 9)
        XCTAssertEqual(ViewerStore.steppedAnnotationId(ids, from: 9, forward: true), 4)
        XCTAssertEqual(ViewerStore.steppedAnnotationId(ids, from: 4, forward: false), 9)
        XCTAssertNil(ViewerStore.steppedAnnotationId(ids, from: 5, forward: true), "a stale selection goes nowhere")
        XCTAssertNil(ViewerStore.steppedAnnotationId([], from: nil, forward: true))
    }

    func testSteppingSelectsTheNextAnnotationAndRevealsItOnItsPage() throws {
        let store = openStore(pages: 3)
        store.applyAnnotationEdit(.addNote(page: 0, rect: PdfRect(x: 10, y: 10, width: 20, height: 20), contents: "a"))
        store.applyAnnotationEdit(.addNote(page: 2, rect: PdfRect(x: 296, y: 386, width: 20, height: 20), contents: "b"))
        let ids = store.annotations.map(\.id)
        XCTAssertTrue(store.canStepAnnotations)

        store.stepAnnotation(forward: true)
        XCTAssertEqual(store.selectedAnnotationId, ids[0])
        XCTAssertEqual(store.pageNavigation?.page, 0)

        store.stepAnnotation(forward: true)
        XCTAssertEqual(store.selectedAnnotationId, ids[1])
        XCTAssertEqual(store.currentPage, 2)
        let anchor = try XCTUnwrap(store.pageNavigation?.anchor)
        // The note's centre (306, 396) is the middle of a 612 × 792 page.
        XCTAssertEqual(anchor.x, 0.5, accuracy: 0.001)
        XCTAssertEqual(anchor.y, 0.5, accuracy: 0.001)

        store.stepAnnotation(forward: true)
        XCTAssertEqual(store.selectedAnnotationId, ids[0], "stepping past the last wraps to the first")
        store.stepAnnotation(forward: false)
        XCTAssertEqual(store.selectedAnnotationId, ids[1])
    }

    func testTheAnchorIsMeasuredFromTheTopOfThePage() throws {
        let store = openStore(pages: 1)
        // Low on the page in PDF space, whose y grows upward.
        store.applyAnnotationEdit(.addNote(page: 0, rect: PdfRect(x: 0, y: 0, width: 612, height: 79.2), contents: "low"))

        store.stepAnnotation(forward: false)

        let anchor = try XCTUnwrap(store.pageNavigation?.anchor)
        XCTAssertEqual(anchor.y, 0.95, accuracy: 0.001)
    }

    func testSteppingIsNotAnEdit() {
        let store = openStore(pages: 1)
        store.applyAnnotationEdit(.addNote(page: 0, rect: PdfRect(x: 10, y: 10, width: 20, height: 20), contents: "a"))
        let revision = store.editRevision

        store.stepAnnotation(forward: true)

        XCTAssertEqual(store.editRevision, revision)
    }

    func testNothingToStepToWithoutAnnotations() {
        let store = openStore(pages: 2)
        XCTAssertFalse(store.canStepAnnotations)

        store.stepAnnotation(forward: true)

        XCTAssertNil(store.selectedAnnotationId)
        XCTAssertNil(store.pageNavigation)
    }

    func testAPlainPageRequestKeepsTheTopAnchor() {
        let store = openStore(pages: 2)
        store.goToPage(1)
        XCTAssertNil(store.pageNavigation?.anchor)
    }

    private func openStore(pages: Int) -> ViewerStore {
        let client = FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: pages))
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return store
    }
}
