import XCTest
@testable import Vitela

/// Edit Content's vector paths (B25 T-206): selecting, moving, reshaping and
/// deleting a frame or a rule — Windows' `PathEditSmoke` and `PathGestures`.
final class ViewerStoreContentPathTests: XCTestCase {
    private let frame = ContentPath(id: 7, page: 0, bbox: PdfRect(x: 100, y: 100, width: 200, height: 100), paint: .stroke)
    private let rule = ContentPath(id: 8, page: 0, bbox: PdfRect(x: 120, y: 150, width: 160, height: 0), paint: .stroke)
    private let panel = ContentPath(id: 9, page: 0, bbox: PdfRect(x: 400, y: 400, width: 100, height: 50), paint: .fill)

    private func withPaths(_ paths: [ContentPath], runs: [ContentTextRun] = []) -> (FakePdfCoreClient, ViewerStore) {
        let (client, store) = ContentEditFixture.opened()
        client.pageContents[0] = PageContent(textRuns: runs, images: [], paths: paths)
        store.setContentEditMode(true)
        ContentEditFixture.read(store)
        return (client, store)
    }

    /// Presses and releases like the shell, then reads the page back as the
    /// overlay does once an edit has landed.
    @discardableResult
    private func drag(_ store: ViewerStore, from: PdfPoint, to: PdfPoint) -> Bool {
        guard store.beginContentPress(page: 0, at: from, zoom: 1) else { return false }
        if case let .move(request) = store.endContentDrag(at: to) {
            store.applyPageWork(store.pageWorkResult(for: request), for: request)
            ContentEditFixture.read(store)
        }
        return true
    }

    func testAClickOnABorderSelectsAndADragMovesItThenItStaysSelected() {
        let (client, store) = withPaths([frame])
        XCTAssertTrue(drag(store, from: PdfPoint(x: 100, y: 150), to: PdfPoint(x: 100, y: 150)))
        XCTAssertEqual(store.selectedContentPath, frame, "a click selects")
        XCTAssertEqual(store.editStatus, ContentEditing.pathSelected)
        XCTAssertTrue(client.contentEdits.isEmpty)

        drag(store, from: PdfPoint(x: 100, y: 150), to: PdfPoint(x: 130, y: 120))

        let moved = PdfRect(x: 130, y: 70, width: 200, height: 100)
        XCTAssertEqual(client.contentEdits, [.movePath(frame, to: moved)])
        XCTAssertEqual(store.editStatus, "Shape moved. Changes are pending save.")
        XCTAssertEqual(store.selectedContentPath?.bbox, moved, "re-selected at its new box")
        XCTAssertTrue(store.canUndo)

        // No save-and-reopen hold, unlike an image: the next drag works.
        drag(store, from: PdfPoint(x: 130, y: 120), to: PdfPoint(x: 140, y: 120))
        XCTAssertEqual(client.contentEdits.last, .movePath(ContentPath(id: 7, page: 0, bbox: moved, paint: .stroke),
                                                           to: PdfRect(x: 140, y: 70, width: 200, height: 100)))
    }

    func testAnOutlineIsItsEdgesAndTextInsideItKeepsTheClick() {
        let cell = ContentEditFixture.run(1, PdfRect(x: 140, y: 140, width: 60, height: 12))
        let (client, store) = withPaths([frame], runs: [cell])
        XCTAssertFalse(store.beginContentPress(page: 0, at: PdfPoint(x: 250, y: 120), zoom: 1), "inside the frame is its cell")
        XCTAssertNil(store.selectedContentPath)
        XCTAssertTrue(store.beginContentPress(page: 0, at: PdfPoint(x: 150, y: 145), zoom: 1))
        XCTAssertEqual(store.contentDrag?.run, cell, "text outranks the frame around it")
        XCTAssertNil(store.selectedContentPath)
        if case .open = store.endContentDrag(at: PdfPoint(x: 150, y: 145)) {} else { XCTFail("the click opens the cell") }
        XCTAssertTrue(client.contentEdits.isEmpty)
    }

    func testAFilledShapeIsHitAnywhereAndTheSmallestHitWins() {
        let (_, store) = withPaths([frame, rule, panel])
        XCTAssertTrue(drag(store, from: PdfPoint(x: 450, y: 425), to: PdfPoint(x: 450, y: 425)))
        XCTAssertEqual(store.selectedContentPath, panel)
        XCTAssertTrue(drag(store, from: PdfPoint(x: 200, y: 152), to: PdfPoint(x: 200, y: 152)), "a flat rule is padded to hit")
        XCTAssertEqual(store.selectedContentPath, rule, "the rule inside the frame, not the frame")
        XCTAssertFalse(drag(store, from: PdfPoint(x: 10, y: 10), to: PdfPoint(x: 10, y: 10)))
        XCTAssertNil(store.selectedContentPath, "a miss lets go of the selection")
    }

    func testASelectedCornerReshapesAndAFlatAxisStaysFlat() {
        let (client, store) = withPaths([frame, rule])
        drag(store, from: PdfPoint(x: 300, y: 130), to: PdfPoint(x: 300, y: 130))
        XCTAssertEqual(store.selectedContentPath, frame)
        drag(store, from: PdfPoint(x: 299, y: 199), to: PdfPoint(x: 340, y: 230))
        XCTAssertEqual(client.contentEdits, [.resizePath(frame, to: PdfRect(x: 100, y: 100, width: 240, height: 130))])
        XCTAssertEqual(store.editStatus, "Shape resized. Changes are pending save.")

        drag(store, from: PdfPoint(x: 200, y: 150), to: PdfPoint(x: 200, y: 150))
        XCTAssertEqual(store.selectedContentPath, rule)
        drag(store, from: PdfPoint(x: 280, y: 150), to: PdfPoint(x: 320, y: 190))
        XCTAssertEqual(client.contentEdits.last, .resizePath(rule, to: PdfRect(x: 120, y: 150, width: 200, height: 0)))
    }

    func testDeleteRemovesTheSelectedPathAndAnUnmovedReleaseRecordsNothing() throws {
        let (client, store) = withPaths([frame])
        XCTAssertNil(store.beginSelectedPathDelete(), "nothing selected")
        drag(store, from: PdfPoint(x: 100, y: 150), to: PdfPoint(x: 100, y: 150))
        let request = try XCTUnwrap(store.beginSelectedPathDelete())
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
        ContentEditFixture.read(store)
        XCTAssertEqual(client.contentEdits, [.removePath(frame)])
        XCTAssertEqual(store.editStatus, "Shape deleted. Changes are pending save.")
        XCTAssertNil(store.selectedContentPath)
        XCTAssertEqual(store.pageContent[0]?.paths, [])
    }

    func testLeavingTheModeDropsThePathSelection() {
        let (_, store) = withPaths([frame])
        drag(store, from: PdfPoint(x: 100, y: 150), to: PdfPoint(x: 100, y: 150))
        store.setContentEditMode(false)
        XCTAssertNil(store.selectedContentPath)
        XCTAssertNil(store.pathDrag)
    }
}
