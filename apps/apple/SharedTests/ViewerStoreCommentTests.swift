import XCTest
@testable import Vitela

final class ViewerStoreCommentTests: XCTestCase {
    private let rect = PdfRect(x: 10, y: 10, width: 20, height: 20)

    func testCommentsAreReadOnOpenAndGroupedByPage() {
        let client = FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: 3))
        client.savedComments = [
            Comment(page: 2, rect: rect, contents: "Third", author: "Ana", date: "2024-03-09", annotationId: nil),
            Comment(page: 0, rect: rect, contents: "First", author: nil, date: nil, annotationId: nil),
        ]
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        XCTAssertEqual(store.commentsByPage.map(\.page), [0, 2])
        XCTAssertEqual(store.commentsByPage.last?.comments.first?.byline, "Ana · 2024-03-09")
        XCTAssertEqual(store.commentsByPage.first?.comments.first?.byline, "")
    }

    func testANoteAddedThisSessionJoinsTheListAndCanBeSelectedFromIt() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        XCTAssertTrue(store.comments.isEmpty)

        store.applyAnnotationEdit(.addNote(page: 0, rect: rect, contents: "Mine"))
        let comment = try XCTUnwrap(store.comments.first)
        XCTAssertEqual(comment.contents, "Mine")

        store.revealComment(comment)
        XCTAssertEqual(store.selectedAnnotationId, comment.annotationId)

        store.undo()
        XCTAssertTrue(store.comments.isEmpty, "the list follows undo")
    }

    func testASavedCommentIsReadNotSelected() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.savedComments = [Comment(page: 0, rect: rect, contents: "Saved", author: nil, date: nil, annotationId: nil)]
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        store.revealComment(store.comments[0])

        XCTAssertNil(store.selectedAnnotationId)
    }
}

extension ViewerStoreCommentTests {
    func testReadingACommentBringsItsPageIntoView() {
        let client = FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: 3))
        client.savedComments = [Comment(page: 2, rect: PdfRect(x: 1, y: 1, width: 1, height: 1), contents: "x", author: nil, date: nil, annotationId: nil)]
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        store.revealComment(store.comments[0])

        XCTAssertEqual(store.pageNavigation?.page, 2)
        XCTAssertEqual(store.currentPage, 2)
    }
}
