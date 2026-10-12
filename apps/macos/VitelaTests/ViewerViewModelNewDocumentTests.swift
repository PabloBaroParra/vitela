import XCTest
@testable import Vitela

final class ViewerViewModelNewDocumentTests: XCTestCase {
    private func makeModel(_ client: FakePdfCoreClient, unsavedChoice: SavePrompts.UnsavedChoice) -> ViewerViewModel {
        let prompts = SavePrompts(
            chooseDestination: { _ in nil },
            confirmSignatureLoss: { false },
            resolveUnsavedChanges: { unsavedChoice }
        )
        return ViewerViewModel(store: ViewerStore(client: client), prompts: prompts)
    }

    func testNewOpensAnUntitledBlankDocument() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = makeModel(client, unsavedChoice: .cancel)

        model.newDocument()

        XCTAssertEqual(model.store.state, .loaded)
        XCTAssertEqual(model.documentName, "Untitled")
        XCTAssertEqual(model.store.pageSlots.count, 1)
    }

    func testUnsavedChangesAreAskedAboutAndCancelKeepsThem() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = makeModel(client, unsavedChoice: .cancel)
        model.store.open(bytes: Data([1]))
        model.store.setDocumentProperty(.title, to: "Edited")

        model.newDocument()

        XCTAssertEqual(client.blankDocumentsCreated, 0)
        XCTAssertTrue(model.store.hasUnsavedChanges)
    }

    func testDiscardingReplacesTheEditedDocument() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = makeModel(client, unsavedChoice: .discard)
        model.store.open(bytes: Data([1]))
        model.store.setDocumentProperty(.title, to: "Edited")

        model.newDocument()

        XCTAssertEqual(client.blankDocumentsCreated, 1)
        XCTAssertFalse(model.store.hasUnsavedChanges)
    }
}
