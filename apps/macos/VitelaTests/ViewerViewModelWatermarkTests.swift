import XCTest
@testable import Vitela

/// Add Watermark…'s sheet: when it opens, and that it stays open until the
/// core has answered, carrying the core's refusal under the fields.
final class ViewerViewModelWatermarkTests: XCTestCase {
    /// What the image picker hands back; `nil` cancels it.
    private var pickedImage: URL?

    private func model(editingAllowed: Bool = true) -> (FakePdfCoreClient, ViewerViewModel) {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792), PageDimensions(width: 612, height: 792)])
        client.editingAllowed = editingAllowed
        let prompts = SavePrompts(
            chooseDestination: { _ in nil }, confirmSignatureLoss: { false }, resolveUnsavedChanges: { .cancel },
            chooseImage: { [unowned self] in self.pickedImage }
        )
        let model = ViewerViewModel(store: ViewerStore(client: client), prompts: prompts, sampleLoader: { Data([1]) })
        model.openSample()
        waitUntil({ model.store.state == .loaded }, "the sample never opened")
        return (client, model)
    }

    func testTheSheetOpensOnlyForADocumentThatAllowsContentChanges() {
        let (_, allowed) = model()
        allowed.requestWatermark()
        XCTAssertTrue(allowed.isWatermarkPresented)

        let (_, forbidden) = model(editingAllowed: false)
        forbidden.requestWatermark()
        XCTAssertFalse(forbidden.isWatermarkPresented)
        XCTAssertEqual(forbidden.store.editStatus, ContentEditing.forbidden)
    }

    func testARecordedStampClosesTheSheet() {
        let (client, model) = model()
        model.requestWatermark()
        var refusal: String?

        XCTAssertNil(model.addWatermark(pages: .all, mark: .text("DRAFT", fontSize: 60), opacityPercent: 30, rotationDegrees: 45, layer: .overContent) { refusal = $0 })

        waitUntil({ !model.isWatermarkPresented }, "the sheet never closed")
        XCTAssertNil(refusal)
        XCTAssertEqual(client.contentEdits.count, 1)
        XCTAssertEqual(model.store.editStatus, "Watermark added to 2 pages. Changes are pending save.")
    }

    func testTheCoresRefusalKeepsTheSheetOpenAndSaysWhy() {
        let (client, model) = model()
        client.contentEditError = EditFailure.failed(Watermarking.encodingGap("下"))
        model.requestWatermark()
        var refusal: String?

        XCTAssertNil(model.addWatermark(pages: .all, mark: .text("下書き", fontSize: 60), opacityPercent: 30, rotationDegrees: 45, layer: .overContent) { refusal = $0 })

        waitUntil({ refusal != nil }, "the refusal never came back")
        XCTAssertEqual(refusal, Watermarking.encodingGap("下"))
        XCTAssertTrue(model.isWatermarkPresented)
    }

    func testARefusalTheStoreCanGiveIsReturnedAtOnce() {
        let (client, model) = model()
        model.requestWatermark()

        XCTAssertEqual(
            model.addWatermark(pages: .range(""), mark: .text("DRAFT", fontSize: 60), opacityPercent: 30, rotationDegrees: 45, layer: .overContent) { _ in },
            Watermarking.blankRange
        )
        XCTAssertTrue(client.contentEdits.isEmpty)
        XCTAssertTrue(model.isWatermarkPresented)
    }

    func testCancelClosesTheSheetAndSaysSo() {
        let (_, model) = model()
        model.requestWatermark()
        model.cancelWatermark()
        XCTAssertFalse(model.isWatermarkPresented)
        XCTAssertEqual(model.store.editStatus, Watermarking.cancelled)
    }

    func testChoosingAPictureReadsItOnceAndNamesIt() throws {
        let (_, model) = model()
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("logo-\(UUID().uuidString).png")
        try Data([0x89, 0x50]).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }

        XCTAssertTrue(model.chooseWatermarkPicture() == (nil, nil), "a cancelled picker says nothing")
        pickedImage = url
        let (picture, problem) = model.chooseWatermarkPicture()
        XCTAssertNil(problem)
        XCTAssertEqual(picture, ViewerViewModel.WatermarkPicture(bytes: Data([0x89, 0x50]), name: url.lastPathComponent))

        pickedImage = url.appendingPathExtension("missing")
        XCTAssertEqual(model.chooseWatermarkPicture().problem, "The selected image could not be read.")
    }
}
