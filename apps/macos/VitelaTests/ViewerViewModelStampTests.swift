import XCTest
@testable import Vitela

final class ViewerViewModelStampTests: XCTestCase {
    private func model(image: URL?, _ client: FakePdfCoreClient) -> ViewerViewModel {
        let prompts = SavePrompts(
            chooseDestination: { _ in nil },
            confirmSignatureLoss: { false },
            resolveUnsavedChanges: { .cancel },
            chooseImage: { image }
        )
        let model = ViewerViewModel(store: ViewerStore(client: client), prompts: prompts)
        model.store.open(bytes: Data([1]))
        return model
    }

    func testThePickedImageIsReadAndInserted() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("stamp-\(UUID().uuidString).png")
        try Data([137, 80, 78, 71, 13, 10, 26, 10, 1]).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = model(image: url, client)
        model.store.beginStampPlacement(page: 0, origin: PdfPoint(x: 10, y: 10), release: PdfPoint(x: 10, y: 10))

        model.resolvePendingStamp()

        waitUntil({ !client.stampInserts.isEmpty }, "the stamp was never inserted")
        XCTAssertNil(model.store.pendingStamp)
    }

    func testCancellingThePickerCancelsThePlacement() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = model(image: nil, client)
        model.store.beginStampPlacement(page: 0, origin: PdfPoint(x: 10, y: 10), release: PdfPoint(x: 10, y: 10))

        model.resolvePendingStamp()

        XCTAssertNil(model.store.pendingStamp)
        XCTAssertEqual(model.store.editStatus, "Stamp placement canceled.")
    }
}
