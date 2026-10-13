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

        XCTAssertEqual(model.contentTextEditor, line)
        XCTAssertNil(model.store.selection, "the click does not select text")
    }

    func testAClickBeforeThePageIsReadReadsItAndSaysSo() {
        let (client, model) = model()
        model.setContentEditMode(true)

        XCTAssertTrue(model.contentClick(page: 0, location: location(150, 699), zoom: 1))

        XCTAssertNil(model.contentTextEditor)
        waitUntil({ client.pageContentReads == [0] }, "the page was never read")
    }

    func testAMissSaysWhatToClick() {
        let (_, model) = model()
        model.setContentEditMode(true)
        readPage(model)

        XCTAssertTrue(model.contentClick(page: 0, location: location(400, 100), zoom: 1))

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
}
