import XCTest
@testable import Vitela

/// Edit Content's images: selecting, moving, resizing, deleting, replacing
/// and inserting — split from `ViewerStoreContentEditTests`; both use
/// `ContentEditFixture`.
final class ViewerStoreContentImageTests: XCTestCase {
    private func run(_ id: UInt64, _ rect: PdfRect, text: String = "Hello") -> ContentTextRun {
        ContentEditFixture.run(id, rect, text: text)
    }

    private func opened() -> (FakePdfCoreClient, ViewerStore) { ContentEditFixture.opened() }

    private func read(_ store: ViewerStore) { ContentEditFixture.read(store) }

    private let photo = ContentImage(id: 40, page: 0, bbox: PdfRect(x: 100, y: 100, width: 200, height: 100), resourceXObjectName: "Im1")

    private func withPhoto(caption: ContentTextRun? = nil) -> (FakePdfCoreClient, ViewerStore) {
        let (client, store) = opened()
        client.pageContents[0] = PageContent(textRuns: caption.map { [$0] } ?? [], images: [photo])
        store.setContentEditMode(true)
        read(store)
        return (client, store)
    }

    private func drag(_ store: ViewerStore, from: PdfPoint, to: PdfPoint) {
        XCTAssertTrue(store.beginContentPress(page: 0, at: from, zoom: 1))
        if case let .move(request) = store.endContentDrag(at: to) {
            store.applyPageWork(store.pageWorkResult(for: request), for: request)
        }
    }

    func testAClickSelectsAnImageAndADragMovesIt() {
        let (client, store) = withPhoto()

        drag(store, from: PdfPoint(x: 150, y: 150), to: PdfPoint(x: 150, y: 150))
        XCTAssertEqual(store.selectedContentImage, photo, "a click selects")
        XCTAssertTrue(client.contentEdits.isEmpty)

        drag(store, from: PdfPoint(x: 150, y: 150), to: PdfPoint(x: 180, y: 120))

        XCTAssertEqual(client.contentEdits, [.moveImage(photo, to: PdfRect(x: 130, y: 70, width: 200, height: 100))])
        XCTAssertEqual(store.editStatus, "Image moved. Changes are pending save.")
        XCTAssertTrue(store.canUndo)
    }

    func testASelectedImagesCornerResizesIt() {
        let (client, store) = withPhoto()
        drag(store, from: PdfPoint(x: 150, y: 150), to: PdfPoint(x: 150, y: 150))

        // The top-right corner, dragged out.
        drag(store, from: PdfPoint(x: 299, y: 199), to: PdfPoint(x: 340, y: 230))

        XCTAssertEqual(client.contentEdits, [.resizeImage(photo, to: PdfRect(x: 100, y: 100, width: 240, height: 130))])
    }

    /// The core folds a second geometry edit into the queued one (B25
    /// T-207), so a moved image stays selected and can be edited again; only
    /// its replace waits for a save.
    func testAnEditedImageCanBeEditedAgainAndOnlyItsReplaceWaits() throws {
        let (client, store) = withPhoto()
        drag(store, from: PdfPoint(x: 150, y: 150), to: PdfPoint(x: 160, y: 150))
        XCTAssertEqual(client.contentEdits.count, 1)
        read(store)
        let moved = store.pageContent[0]!.images[0]
        XCTAssertEqual(store.selectedContentImage, moved, "the frame follows the image")
        XCTAssertFalse(store.canReplaceSelectedImage)
        XCTAssertEqual(store.selectedImageReplacementRefusal(), ViewerStore.imageAgain)

        drag(store, from: PdfPoint(x: 170, y: 150), to: PdfPoint(x: 170, y: 170))
        XCTAssertEqual(client.contentEdits.last, .moveImage(moved, to: PdfRect(x: 110, y: 120, width: 200, height: 100)))
        read(store)
        let request = try XCTUnwrap(store.beginSelectedImageDelete())
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
        XCTAssertEqual(client.contentEdits.count, 3)

        store.undo()
        if let refresh = store.beginPageRefresh() {
            store.applyPageWork(store.pageWorkResult(for: refresh), for: refresh)
        }
        XCTAssertTrue(store.contentEdit.editedImages.isEmpty)
    }

    func testDeletingTheSelectedImage() throws {
        let (client, store) = withPhoto()
        drag(store, from: PdfPoint(x: 150, y: 150), to: PdfPoint(x: 150, y: 150))

        let request = try XCTUnwrap(store.beginSelectedImageDelete())
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertEqual(client.contentEdits, [.removeImage(photo)])
        XCTAssertEqual(store.editStatus, "Image deleted. Changes are pending save.")
        XCTAssertNil(store.selectedContentImage)
    }

    func testTextThePressIsInsideWinsOverTheImageUnderIt() {
        let caption = run(9, PdfRect(x: 120, y: 120, width: 60, height: 12), text: "Caption")
        let (_, store) = withPhoto(caption: caption)

        XCTAssertTrue(store.beginContentPress(page: 0, at: PdfPoint(x: 130, y: 125), zoom: 1))
        XCTAssertEqual(store.contentDrag?.run, caption)
        XCTAssertNil(store.selectedContentImage)

        store.cancelContentDrag()
        XCTAssertTrue(store.beginContentPress(page: 0, at: PdfPoint(x: 250, y: 180), zoom: 1))
        XCTAssertEqual(store.selectedContentImage, photo, "elsewhere on the photo, the photo")
    }

    // MARK: - Replacing and inserting images

    func testAnImageTheCoreCannotReadBackSaysSoWhenSelected() {
        let (client, store) = withPhoto()
        client.imageReplaceRefusal = ContentEditing.replaceRefused
        drag(store, from: PdfPoint(x: 150, y: 150), to: PdfPoint(x: 150, y: 150))

        XCTAssertEqual(store.editStatus, ViewerStore.imageSelectedNoReplace)
        XCTAssertFalse(store.canReplaceSelectedImage)
    }

    func testAReplaceableImageIsOfferedUntilItIsEdited() {
        let (_, store) = withPhoto()
        drag(store, from: PdfPoint(x: 150, y: 150), to: PdfPoint(x: 150, y: 150))
        XCTAssertTrue(store.canReplaceSelectedImage)

        drag(store, from: PdfPoint(x: 150, y: 150), to: PdfPoint(x: 180, y: 120))
        XCTAssertFalse(store.canReplaceSelectedImage, "held since the move, as on Linux and Windows")
    }

    func testReplacingIsAskedFirstAndTheImageIsHeldAfter() throws {
        let (client, store) = withPhoto()
        drag(store, from: PdfPoint(x: 150, y: 150), to: PdfPoint(x: 150, y: 150))
        client.imageReplaceRefusal = ContentEditing.replaceRefused
        XCTAssertEqual(store.selectedImageReplacementRefusal(), ContentEditing.replaceRefused)

        client.imageReplaceRefusal = nil
        XCTAssertNil(store.selectedImageReplacementRefusal())
        let request = try XCTUnwrap(store.beginSelectedImageReplace(with: Data([1, 2])))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertEqual(client.contentEdits, [.replaceImage(photo, with: Data([1, 2]))])
        XCTAssertEqual(store.editStatus, "Image replaced. Changes are pending save.")
        XCTAssertTrue(store.contentEdit.editedImages.contains(ContentImageKey(page: 0, id: photo.id)))
    }

    func testANewImageIsSizedByTheCoresStampPolicyAtTheClick() throws {
        let (client, store) = opened()
        store.setContentEditMode(true)

        let request = try XCTUnwrap(store.beginImageInsert(page: 0, at: PdfPoint(x: 72, y: 700), bytes: Data([9])))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        // The fake's stamp policy: 100 × 50, hung from the click.
        XCTAssertEqual(client.contentEdits, [.insertImage(page: 0, rect: PdfRect(x: 72, y: 650, width: 100, height: 50), bytes: Data([9]))])
        XCTAssertEqual(store.editStatus, "Image inserted. Changes are pending save.")
        XCTAssertTrue(store.canUndo)
    }
}
