import XCTest
@testable import Vitela

final class ViewerStoreContentEditTests: XCTestCase {
    private func run(_ id: UInt64, _ rect: PdfRect, text: String = "Hello", kind: ContentFontKind = .standard14, page: Int = 0) -> ContentTextRun {
        ContentTextRun(id: id, page: page, bbox: rect, resourceFontName: "F1", fontKind: kind, text: text)
    }

    private func opened(_ client: FakePdfCoreClient = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])) -> (FakePdfCoreClient, ViewerStore) {
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return (client, store)
    }

    /// Reads page 0 the way the shell does, synchronously.
    private func read(_ store: ViewerStore, page: Int = 0) {
        guard let request = store.beginPageContent(page: page) else { return }
        store.applyPageContent(store.pageContentResult(for: request), for: request)
    }

    private func perform(_ store: ViewerStore, _ edit: ContentEdit) {
        guard let request = store.beginContentEdit(edit) else { return }
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
    }

    // MARK: - Hit-testing

    func testAClickInsideARunWinsAndTheSmallestOfEqualsIsChosen() {
        let wide = run(1, PdfRect(x: 0, y: 0, width: 300, height: 20))
        let word = run(2, PdfRect(x: 10, y: 2, width: 40, height: 12))
        XCTAssertEqual(ContentEditing.textRun(at: PdfPoint(x: 20, y: 8), in: [wide, word], reach: 0)?.id, 2)
        XCTAssertEqual(ContentEditing.textRun(at: PdfPoint(x: 200, y: 8), in: [wide, word], reach: 0)?.id, 1)
    }

    func testANearMissWithinReachStillMeansTheRun() {
        let line = run(1, PdfRect(x: 100, y: 100, width: 50, height: 10))
        XCTAssertEqual(ContentEditing.textRun(at: PdfPoint(x: 120, y: 113), in: [line], reach: 4)?.id, 1)
        XCTAssertNil(ContentEditing.textRun(at: PdfPoint(x: 120, y: 120), in: [line], reach: 4))
    }

    func testReachIsInScreenPointsSoItShrinksWhenZoomedIn() {
        let (client, store) = opened()
        client.pageContents[0] = PageContent(textRuns: [run(1, PdfRect(x: 100, y: 100, width: 50, height: 10))], images: [])
        store.setContentEditMode(true)
        read(store)

        XCTAssertNotNil(store.textRun(page: 0, at: PdfPoint(x: 120, y: 114), zoom: 1))
        XCTAssertNil(store.textRun(page: 0, at: PdfPoint(x: 120, y: 114), zoom: 4))
    }

    // MARK: - The mode

    func testTheModeNeedsPermission() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.editingAllowed = false
        let (_, store) = opened(client)

        store.setContentEditMode(true)

        XCTAssertFalse(store.contentEditMode)
        XCTAssertEqual(store.editStatus, ContentEditing.forbidden)
    }

    func testOneModeOwnsAClickAtATime() {
        let (_, store) = opened()
        store.armAnnotationTool(.highlight)

        store.setContentEditMode(true)
        XCTAssertNil(store.armedTool)
        XCTAssertEqual(store.editStatus, ContentEditing.armed)

        store.armAnnotationTool(.shape)
        XCTAssertFalse(store.contentEditMode, "arming a tool leaves Edit Text")
    }

    func testAPageIsReadOnceAndOnlyInTheMode() {
        let (client, store) = opened()
        XCTAssertNil(store.beginPageContent(page: 0), "not outside the mode")

        store.setContentEditMode(true)
        read(store)
        read(store)

        XCTAssertEqual(client.pageContentReads, [0])
    }

    func testAReadOfAnEarlierLayoutIsDropped() throws {
        let (client, store) = opened()
        client.pageContents[0] = PageContent(textRuns: [run(1, PdfRect(x: 0, y: 0, width: 10, height: 10))], images: [])
        store.setContentEditMode(true)
        let request = try XCTUnwrap(store.beginPageContent(page: 0))
        store.bumpGeneration()

        store.applyPageContent(store.pageContentResult(for: request), for: request)

        XCTAssertNil(store.pageContent[0])
    }

    // MARK: - Editing

    func testRetypingIsOneUndoableStepAndThePageIsReadAgain() {
        let (client, store) = opened()
        let line = run(7, PdfRect(x: 10, y: 10, width: 80, height: 12))
        client.pageContents[0] = PageContent(textRuns: [line], images: [])
        store.setContentEditMode(true)
        read(store)

        perform(store, .retypeText(line, to: "Goodbye"))

        XCTAssertEqual(client.contentEdits, [.retypeText(line, to: "Goodbye")])
        XCTAssertEqual(store.editStatus, "Text updated. Changes are pending save.")
        XCTAssertTrue(store.canUndo)
        XCTAssertTrue(store.hasUnsavedChanges)
        XCTAssertNil(store.pageContent[0], "what the page paints changed")
        read(store)
        XCTAssertEqual(store.pageContent[0]?.textRuns.first?.text, "Goodbye")
    }

    func testDeletingRemovesTheRun() {
        let (client, store) = opened()
        let line = run(7, PdfRect(x: 10, y: 10, width: 80, height: 12))
        client.pageContents[0] = PageContent(textRuns: [line], images: [])
        store.setContentEditMode(true)

        perform(store, .removeText(line))

        XCTAssertEqual(store.editStatus, "Text deleted. Changes are pending save.")
        read(store)
        XCTAssertEqual(store.pageContent[0]?.textRuns, [])
    }

    func testAnEmptyOrUnchangedRetypeIsNotSent() {
        let (client, store) = opened()
        let line = run(7, PdfRect(x: 10, y: 10, width: 80, height: 12))
        store.setContentEditMode(true)

        XCTAssertNil(store.beginContentEdit(.retypeText(line, to: "  ")))
        XCTAssertEqual(store.editStatus, ContentEditing.emptyRetype)
        XCTAssertNil(store.beginContentEdit(.retypeText(line, to: "Hello")))
        XCTAssertTrue(client.contentEdits.isEmpty)
        XCTAssertFalse(store.pageWorkInFlight)
    }

    func testARefusalSaysWhyAndRecordsNothing() {
        let (client, store) = opened()
        client.contentEditError = EditFailure.failed("This text's font cannot show \"€\". Try different characters.")
        let line = run(7, PdfRect(x: 10, y: 10, width: 80, height: 12))
        store.setContentEditMode(true)

        perform(store, .retypeText(line, to: "€5"))

        XCTAssertEqual(store.editStatus, "This text's font cannot show \"€\". Try different characters.")
        XCTAssertFalse(store.canUndo)
        XCTAssertFalse(store.pageWorkInFlight)
    }

    func testACompositeFontRunSaysItsFontWillBeReplaced() {
        XCTAssertTrue(run(1, PdfRect(x: 0, y: 0, width: 1, height: 1), kind: .embeddedComposite).substitutesFont)
        XCTAssertFalse(run(1, PdfRect(x: 0, y: 0, width: 1, height: 1), kind: .embeddedSimple).substitutesFont)
    }

    func testOpeningAnotherDocumentLeavesTheMode() {
        let (_, store) = opened()
        store.setContentEditMode(true)

        store.open(bytes: Data([2]))

        XCTAssertFalse(store.contentEditMode)
        XCTAssertTrue(store.pageContent.isEmpty)
    }

    // MARK: - Moving

    func testAPressIsAClickUntilItTravelsFourScreenPoints() throws {
        let (client, store) = opened()
        let line = run(7, PdfRect(x: 10, y: 10, width: 80, height: 12))
        client.pageContents[0] = PageContent(textRuns: [line], images: [])
        store.setContentEditMode(true)
        read(store)

        XCTAssertEqual(store.beginContentDrag(page: 0, at: PdfPoint(x: 20, y: 15), zoom: 2), line)
        store.continueContentDrag(to: PdfPoint(x: 21.5, y: 15))
        XCTAssertEqual(store.contentDrag?.moving, false, "three screen points at 2×")
        store.continueContentDrag(to: PdfPoint(x: 22, y: 15))
        XCTAssertEqual(store.contentDrag?.moving, true, "four screen points at 2×")
    }

    func testAClickOpensTheLineAndADragMovesIt() throws {
        let (client, store) = opened()
        let line = run(7, PdfRect(x: 10, y: 10, width: 80, height: 12))
        client.pageContents[0] = PageContent(textRuns: [line], images: [])
        store.setContentEditMode(true)
        read(store)

        _ = store.beginContentDrag(page: 0, at: PdfPoint(x: 20, y: 15), zoom: 1)
        guard case let .open(opened) = store.endContentDrag(at: PdfPoint(x: 21, y: 15)) else { return XCTFail("a click opens") }
        XCTAssertEqual(opened, line)

        _ = store.beginContentDrag(page: 0, at: PdfPoint(x: 20, y: 15), zoom: 1)
        guard case let .move(request) = store.endContentDrag(at: PdfPoint(x: 120, y: 215)) else { return XCTFail("a drag moves") }
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertEqual(client.contentEdits, [.moveText(line, to: PdfRect(x: 110, y: 210, width: 80, height: 12))])
        XCTAssertTrue(store.canUndo)
        read(store)
        XCTAssertEqual(store.pageContent[0]?.textRuns.first?.bbox, PdfRect(x: 110, y: 210, width: 80, height: 12))
    }

    func testADragBackToWhereItStartedChangesNothing() {
        let (client, store) = opened()
        let line = run(7, PdfRect(x: 10, y: 10, width: 80, height: 12))
        client.pageContents[0] = PageContent(textRuns: [line], images: [])
        store.setContentEditMode(true)
        read(store)

        _ = store.beginContentDrag(page: 0, at: PdfPoint(x: 20, y: 15), zoom: 1)
        store.continueContentDrag(to: PdfPoint(x: 60, y: 15))
        guard case .nothing = store.endContentDrag(at: PdfPoint(x: 20, y: 15)) else { return XCTFail("nothing to move") }

        XCTAssertEqual(store.editStatus, ContentEditing.positionUnchanged)
        XCTAssertTrue(client.contentEdits.isEmpty)
    }

    // MARK: - Inserting

    func testANewLineGoesWithItsTopLeftCornerAtTheClick() throws {
        let (client, store) = opened()
        store.setContentEditMode(true)

        let (problem, request) = store.beginTextInsert(page: 0, at: PdfPoint(x: 72, y: 700), text: "Paid", size: "14")
        XCTAssertNil(problem)
        let work = try XCTUnwrap(request)
        store.applyPageWork(store.pageWorkResult(for: work), for: work)

        XCTAssertEqual(client.contentEdits, [.insertText(page: 0, rect: PdfRect(x: 72, y: 686, width: 14, height: 14), text: "Paid")])
        XCTAssertEqual(store.editStatus, "Text inserted. Changes are pending save.")
        XCTAssertTrue(store.canUndo)
    }

    func testAnInsertNeedsOneLineAndASizeFromOneToSeventyTwo() {
        let (client, store) = opened()
        store.setContentEditMode(true)
        let at = PdfPoint(x: 10, y: 10)

        XCTAssertEqual(store.beginTextInsert(page: 0, at: at, text: "  ", size: "14").problem, ContentEditing.insertEmpty)
        XCTAssertEqual(store.beginTextInsert(page: 0, at: at, text: "two\nlines", size: "14").problem, ContentEditing.insertEmpty)
        XCTAssertEqual(store.beginTextInsert(page: 0, at: at, text: "ok", size: "0").problem, ContentEditing.insertSizeInvalid)
        XCTAssertEqual(store.beginTextInsert(page: 0, at: at, text: "ok", size: "73").problem, ContentEditing.insertSizeInvalid)
        XCTAssertEqual(store.beginTextInsert(page: 0, at: at, text: "ok", size: "big").problem, ContentEditing.insertSizeInvalid)
        XCTAssertTrue(client.contentEdits.isEmpty)
        XCTAssertFalse(store.pageWorkInFlight)
    }

    // MARK: - Images

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

    func testAnEditedImageIsHeldUntilReopenedAndAnUndoGivesItBack() {
        let (client, store) = withPhoto()
        drag(store, from: PdfPoint(x: 150, y: 150), to: PdfPoint(x: 160, y: 150))
        XCTAssertEqual(client.contentEdits.count, 1)
        read(store)
        let moved = store.pageContent[0]!.images[0]

        XCTAssertTrue(store.beginContentPress(page: 0, at: PdfPoint(x: 170, y: 150), zoom: 1))
        XCTAssertEqual(store.editStatus, ViewerStore.imageAgain)
        XCTAssertEqual(store.selectedContentImage, moved)
        XCTAssertNil(store.imageDrag)
        XCTAssertNil(store.beginSelectedImageDelete())

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
}
