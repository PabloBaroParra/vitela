import XCTest
@testable import Vitela

final class ViewerViewModelContentEditTests: XCTestCase {
    private let line = ContentTextRun(
        id: 3, page: 0, bbox: PdfRect(x: 100, y: 692, width: 120, height: 14), resourceFontName: "F1", fontKind: .standard14, text: "Invoice"
    )

    /// What the image picker hands back; `nil` cancels it.
    private var pickedImage: URL?

    private func model() -> (FakePdfCoreClient, ViewerViewModel) {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.pageContents[0] = PageContent(textRuns: [line], images: [])
        let prompts = SavePrompts(
            chooseDestination: { _ in nil }, confirmSignatureLoss: { false }, resolveUnsavedChanges: { .cancel },
            chooseImage: { [unowned self] in self.pickedImage }
        )
        let model = ViewerViewModel(store: ViewerStore(client: client), prompts: prompts, sampleLoader: { Data([1]) })
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

    func testAnImageIsSelectedDraggedAndDeletedFromTheView() {
        let (client, model) = model()
        let photo = ContentImage(id: 5, page: 0, bbox: PdfRect(x: 300, y: 300, width: 100, height: 100), resourceXObjectName: "Im1")
        client.pageContents[0] = PageContent(textRuns: [line], images: [photo])
        model.setContentEditMode(true)
        readPage(model)
        let size = PageDimensions(width: 612, height: 792)

        model.pageDragBegan(page: 0, location: location(350, 350), dimensions: size, zoom: 1)
        model.pageDragEnded(page: 0, location: location(350, 350), zoom: 1)
        XCTAssertEqual(model.store.selectedContentImage, photo)

        model.deleteSelectedImage()
        waitUntil({ !model.store.pageWorkInFlight && client.contentEdits == [.removeImage(photo)] }, "the delete never ran")
    }

    private func imageFile(_ bytes: [UInt8]) throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("vitela-image-\(UUID().uuidString).png")
        try Data(bytes).write(to: url)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    func testAddImagePicksAFileThenTheClickPlacesIt() throws {
        let (client, model) = model()
        pickedImage = try imageFile([7, 7])

        model.armImageInsert()
        XCTAssertTrue(model.store.contentEditMode)
        XCTAssertEqual(model.store.editStatus, ContentEditing.imageInsertPrompt)
        model.pageDragBegan(page: 0, location: location(72, 300), dimensions: PageDimensions(width: 612, height: 792), zoom: 1)

        XCTAssertNil(model.imageInsertBytes)
        waitUntil({ !model.store.pageWorkInFlight && client.contentEdits.count == 1 }, "the insertion never ran")
        XCTAssertEqual(client.contentEdits.first, .insertImage(page: 0, rect: PdfRect(x: 72, y: 250, width: 100, height: 50), bytes: Data([7, 7])))
    }

    func testCancellingThePickerArmsNothing() {
        let (_, model) = model()
        pickedImage = nil

        model.armImageInsert()

        XCTAssertNil(model.imageInsertBytes)
        XCTAssertFalse(model.store.contentEditMode)
    }

    func testReplaceAsksTheCoreBeforeThePicker() throws {
        let (client, model) = model()
        let photo = ContentImage(id: 5, page: 0, bbox: PdfRect(x: 300, y: 300, width: 100, height: 100), resourceXObjectName: "Im1")
        client.pageContents[0] = PageContent(textRuns: [], images: [photo])
        model.setContentEditMode(true)
        readPage(model)
        model.pageDragBegan(page: 0, location: location(350, 350), dimensions: PageDimensions(width: 612, height: 792), zoom: 1)
        model.pageDragEnded(page: 0, location: location(350, 350), zoom: 1)
        client.imageReplaceRefusal = ContentEditing.replaceRefused
        pickedImage = try imageFile([1])

        model.replaceSelectedImage()
        XCTAssertEqual(model.store.editStatus, ContentEditing.replaceRefused)
        XCTAssertTrue(client.contentEdits.isEmpty)

        client.imageReplaceRefusal = nil
        model.replaceSelectedImage()
        waitUntil({ !model.store.pageWorkInFlight && client.contentEdits == [.replaceImage(photo, with: Data([1]))] }, "the replace never ran")
    }

    func testTheEditTextSheetsMoveClosesItAndMovesTheLine() {
        let (client, model) = model()
        model.setContentEditMode(true)
        readPage(model)
        model.contentTextEditor = line

        XCTAssertEqual(model.moveText(line, toX: .infinity, y: 0), ViewerStore.ContentGeometry.invalidPosition)
        XCTAssertEqual(model.contentTextEditor, line, "a bad number keeps the sheet open")
        XCTAssertNil(model.moveText(line, toX: 200, y: 600))

        XCTAssertNil(model.contentTextEditor)
        waitUntil({ client.contentEdits == [.moveText(line, to: PdfRect(x: 200, y: 600, width: 120, height: 14))] }, "the line never moved")
    }
}
