import XCTest
@testable import Vitela

/// Home's Tools grid: a tile goes to its tool, and one picked with no
/// document open waits for the document chosen for it.
final class ViewerViewModelHomeToolsTests: XCTestCase {
    private func model(open: Bool = true) -> (FakePdfCoreClient, ViewerViewModel) {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client), sampleLoader: { Data([1]) })
        if open {
            model.openSample()
            waitUntil({ model.store.state == .loaded }, "the sample never opened")
        }
        return (client, model)
    }

    func testTheGridIsLinuxsSixToolsInItsOrder() {
        XCTAssertEqual(HomeTool.allCases.map(\.label), ["Edit", "Annotate", "Sign", "Organize", "Compress", "Protect"])
    }

    func testWithADocumentOpenATileGoesStraightToItsTool() {
        let (_, model) = model()

        model.openHomeTool(.organize)
        XCTAssertTrue(model.isOrganizing)
        model.isOrganizing = false

        model.openHomeTool(.edit)
        XCTAssertTrue(model.store.contentEditMode)

        model.openHomeTool(.annotate)
        XCTAssertEqual(model.store.editStatus, HomeTool.annotatePrompt)
        XCTAssertNil(model.store.armedTool, "a tile navigates; it never arms a mark for the reader")

        model.openHomeTool(.compress)
        XCTAssertTrue(model.isCompressPresented)
        model.isCompressPresented = false

        model.openHomeTool(.protect)
        XCTAssertTrue(model.isProtectPresented)
        XCTAssertNil(model.pendingHomeTool)
    }

    func testAToolPickedFromAColdStartLandsOnceItsDocumentLoads() {
        let (_, model) = model(open: false)
        model.pendingHomeTool = .organize

        model.openSample()

        waitUntil({ model.isOrganizing }, "the pending tool never landed")
        XCTAssertNil(model.pendingHomeTool)
    }

    func testAPendingToolIsDroppedWhenTheOpenFails() {
        let (_, model) = model(open: false)
        model.pendingHomeTool = .edit

        model.applyPendingHomeTool(.error(.openFailed("Not a PDF.")))

        XCTAssertNil(model.pendingHomeTool)
    }

    func testAPendingToolWaitsThroughAPasswordPrompt() {
        let (_, model) = model(open: false)
        model.pendingHomeTool = .sign

        model.applyPendingHomeTool(.error(.passwordRequired))

        XCTAssertEqual(model.pendingHomeTool, .sign)
    }
}
