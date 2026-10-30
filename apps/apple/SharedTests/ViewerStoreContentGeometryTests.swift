import XCTest
@testable import Vitela

/// Typed positions and sizes for page content — Windows' Move / Resize
/// image and Move text, over the same edits a drag records.
final class ViewerStoreContentGeometryTests: XCTestCase {
    private let photo = ContentImage(id: 40, page: 0, bbox: PdfRect(x: 100, y: 100, width: 200, height: 100), resourceXObjectName: "Im1")
    private let line = ContentEditFixture.run(3, PdfRect(x: 72, y: 700, width: 120, height: 14))

    /// Edit Content on, the photo and the line read, the photo selected.
    private func withPhotoSelected() -> (FakePdfCoreClient, ViewerStore) {
        let (client, store) = ContentEditFixture.opened()
        client.pageContents[0] = PageContent(textRuns: [line], images: [photo])
        store.setContentEditMode(true)
        ContentEditFixture.read(store)
        XCTAssertTrue(store.beginContentPress(page: 0, at: PdfPoint(x: 150, y: 150), zoom: 1))
        _ = store.endContentDrag(at: PdfPoint(x: 150, y: 150))
        XCTAssertEqual(store.selectedContentImage, photo)
        return (client, store)
    }

    private func run(_ store: ViewerStore, _ answer: (problem: String?, request: ViewerStore.PageWorkRequest?)) throws {
        XCTAssertNil(answer.problem)
        let request = try XCTUnwrap(answer.request)
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
    }

    func testAnImageMovesToTypedCoordinatesWithItsSizeKept() throws {
        let (client, store) = withPhotoSelected()

        try run(store, store.beginSelectedImageMove(toX: 36, y: 500))

        XCTAssertEqual(client.contentEdits, [.moveImage(photo, to: PdfRect(x: 36, y: 500, width: 200, height: 100))])
        XCTAssertEqual(store.editStatus, "Image moved. Changes are pending save.")
    }

    func testAnImageTakesATypedSizeFromItsOrigin() throws {
        let (client, store) = withPhotoSelected()

        try run(store, store.beginSelectedImageResize(width: 50, height: 400))

        XCTAssertEqual(client.contentEdits, [.resizeImage(photo, to: PdfRect(x: 100, y: 100, width: 50, height: 400))])
    }

    func testTheSameNumbersRecordNothingAndBadOnesKeepTheSheetOpen() {
        let (client, store) = withPhotoSelected()

        let same = store.beginSelectedImageMove(toX: 100, y: 100)
        XCTAssertNil(same.problem)
        XCTAssertNil(same.request)
        XCTAssertEqual(store.editStatus, ViewerStore.ContentGeometry.imagePositionUnchanged)
        XCTAssertNil(store.beginSelectedImageResize(width: 200, height: 100).request)
        XCTAssertEqual(store.editStatus, ViewerStore.ContentGeometry.imageSizeUnchanged)

        XCTAssertEqual(store.beginSelectedImageMove(toX: .nan, y: 0).problem, ViewerStore.ContentGeometry.invalidPosition)
        XCTAssertEqual(store.beginSelectedImageResize(width: 0, height: 5).problem, ViewerStore.ContentGeometry.invalidSize)
        XCTAssertTrue(client.contentEdits.isEmpty)
    }

    func testAnImageEditedSinceOpeningIsHeldAsADragHoldsIt() throws {
        let (_, store) = withPhotoSelected()
        try run(store, store.beginSelectedImageMove(toX: 36, y: 500))

        // The relayout drops the selection; select the moved image again.
        store.contentEdit.selectedImage = photo
        XCTAssertEqual(store.beginSelectedImageResize(width: 10, height: 10).problem, ViewerStore.imageAgain)
    }

    func testALineMovesToTypedCoordinates() throws {
        let (client, store) = withPhotoSelected()

        try run(store, store.beginTextMove(ContentTextBlock(line), toX: 300, y: 650))

        XCTAssertEqual(client.contentEdits, [.moveText(line, to: PdfRect(x: 300, y: 650, width: 120, height: 14))])
        XCTAssertEqual(store.editStatus, "Text moved. Changes are pending save.")
    }

    func testALineLeftWhereItIsSaysSo() {
        let (client, store) = withPhotoSelected()

        XCTAssertNil(store.beginTextMove(ContentTextBlock(line), toX: 72, y: 700).request)

        XCTAssertEqual(store.editStatus, ContentEditing.positionUnchanged)
        XCTAssertTrue(client.contentEdits.isEmpty)
    }
}
