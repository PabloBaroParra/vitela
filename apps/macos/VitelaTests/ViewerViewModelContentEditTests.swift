import XCTest
@testable import Vitela

final class ViewerViewModelContentEditTests: XCTestCase {
    private let line = ContentTextRun(
        id: 3, page: 0, bbox: PdfRect(x: 100, y: 692, width: 120, height: 14), resourceFontName: "F1", fontKind: .standard14, text: "Invoice"
    )

    private func model() -> (FakePdfCoreClient, ViewerViewModel) {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.pageContents[0] = PageContent(textRuns: [line], images: [])
        let model = ViewerViewModel(store: ViewerStore(client: client), sampleLoader: { Data([1]) })
        model.openSample()
        waitUntil({ model.store.state == .loaded }, "the sample never opened")
        return (client, model)
    }

    /// Where `point` (page space) is drawn at zoom 1: y flips.
    private func location(_ x: Double, _ y: Double) -> CGPoint { CGPoint(x: x, y: 792 - y) }

    private func readPage(_ model: ViewerViewModel) {
        model.readPageContent(page: 0)
        waitUntil({ model.store.pageContent[0] != nil }, "the page was never read")
    }

    func testAClickOnALineOpensItsEditor() {
        let (_, model) = model()
        model.setContentEditMode(true)
        readPage(model)

        model.pageDragBegan(page: 0, location: location(150, 699), dimensions: PageDimensions(width: 612, height: 792), zoom: 1)
        model.pageDragChanged(page: 0, location: location(151, 699), dimensions: PageDimensions(width: 612, height: 792), zoom: 1)
        model.pageDragEnded(page: 0, location: location(151, 699), zoom: 1)

        XCTAssertEqual(model.contentTextEditor, line, "a press that barely moves is a click")
        XCTAssertNil(model.store.selection, "the click does not select text")
    }

    func testAClickBeforeThePageIsReadReadsItAndSaysSo() {
        let (client, model) = model()
        model.setContentEditMode(true)

        XCTAssertTrue(model.contentPress(page: 0, location: location(150, 699), zoom: 1))

        XCTAssertNil(model.contentTextEditor)
        waitUntil({ client.pageContentReads == [0] }, "the page was never read")
    }

    func testAMissSaysWhatToClick() {
        let (_, model) = model()
        model.setContentEditMode(true)
        readPage(model)

        XCTAssertTrue(model.contentPress(page: 0, location: location(400, 100), zoom: 1))

        XCTAssertNil(model.contentTextEditor)
        XCTAssertEqual(model.store.editStatus, ContentEditing.missed)
    }

    func testSavingAndDeletingRunAsPageWork() {
        let (client, model) = model()
        model.setContentEditMode(true)
        readPage(model)

        model.retypeText(line, to: "Receipt")
        waitUntil({ !model.store.pageWorkInFlight && client.contentEdits.count == 1 }, "the retype never ran")
        XCTAssertEqual(client.contentEdits.first, .retypeText(line, to: "Receipt"))
        XCTAssertNil(model.contentTextEditor)

        model.deleteText(line)
        waitUntil({ !model.store.pageWorkInFlight && client.contentEdits.count == 2 }, "the delete never ran")
        XCTAssertEqual(client.contentEdits.last, .removeText(line))
    }

    func testTheModeIsNotOfferedOverOrganize() {
        let (_, model) = model()
        model.isOrganizing = true

        model.setContentEditMode(true)

        XCTAssertFalse(model.store.contentEditMode)
    }

    func testCancellingSaysSoAndLeavesThePage() {
        let (client, model) = model()
        model.setContentEditMode(true)
        model.contentTextEditor = line

        model.cancelTextEdit()

        XCTAssertNil(model.contentTextEditor)
        XCTAssertEqual(model.store.editStatus, "Edit cancelled.")
        XCTAssertTrue(client.contentEdits.isEmpty)
    }

    func testDraggingALineMovesItByTheDrag() {
        let (client, model) = model()
        model.setContentEditMode(true)
        readPage(model)
        let size = PageDimensions(width: 612, height: 792)

        model.pageDragBegan(page: 0, location: location(150, 699), dimensions: size, zoom: 1)
        model.pageDragChanged(page: 0, location: location(170, 649), dimensions: size, zoom: 1)
        XCTAssertEqual(model.store.contentDrag?.moving, true)
        model.pageDragEnded(page: 0, location: location(180, 599), zoom: 1)

        waitUntil({ !model.store.pageWorkInFlight && client.contentEdits.count == 1 }, "the move never ran")
        XCTAssertEqual(client.contentEdits.first, .moveText(line, to: PdfRect(x: 130, y: 592, width: 120, height: 14)))
        XCTAssertNil(model.contentTextEditor, "a drag does not open the line")
        XCTAssertEqual(model.store.editStatus, "Text moved. Changes are pending save.")
    }

    func testAMovedLineCannotBeRetypedUntilSaved() {
        let (client, model) = model()
        client.movedRuns = [line.id]
        model.setContentEditMode(true)
        readPage(model)
        let size = PageDimensions(width: 612, height: 792)

        model.pageDragBegan(page: 0, location: location(150, 699), dimensions: size, zoom: 1)
        model.pageDragEnded(page: 0, location: location(150, 699), zoom: 1)

        XCTAssertNil(model.contentTextEditor)
        XCTAssertEqual(model.store.editStatus, ContentEditing.movedNotSaved)
    }

    func testAddTextArmsTheModeAndTheNextClickPlacesTheLine() {
        let (client, model) = model()

        model.armTextInsert(true)
        XCTAssertTrue(model.store.contentEditMode, "Add Text turns Edit Text on")
        XCTAssertEqual(model.store.editStatus, ContentEditing.insertPrompt)
        model.pageDragBegan(page: 0, location: location(72, 500), dimensions: PageDimensions(width: 612, height: 792), zoom: 1)

        XCTAssertFalse(model.textInsertArmed)
        XCTAssertEqual(model.pendingTextInsert, PendingTextInsert(page: 0, point: PdfPoint(x: 72, y: 500)))
        XCTAssertEqual(model.insertText("x", size: "200"), ContentEditing.insertSizeInvalid, "the sheet stays open")
        XCTAssertNil(model.insertText("Approved", size: "12"))
        XCTAssertNil(model.pendingTextInsert)
        waitUntil({ !model.store.pageWorkInFlight && client.contentEdits.count == 1 }, "the insertion never ran")
        XCTAssertEqual(client.contentEdits.first, .insertText(page: 0, rect: PdfRect(x: 72, y: 488, width: 12, height: 12), text: "Approved"))
    }

    func testCancellingAnInsertSaysSo() {
        let (client, model) = model()
        model.armTextInsert(true)
        model.armTextInsert(false)
        XCTAssertEqual(model.store.editStatus, ContentEditing.insertCancelled)

        model.pendingTextInsert = PendingTextInsert(page: 0, point: PdfPoint(x: 1, y: 1))
        model.cancelTextInsert()

        XCTAssertNil(model.pendingTextInsert)
        XCTAssertTrue(client.contentEdits.isEmpty)
    }
}
