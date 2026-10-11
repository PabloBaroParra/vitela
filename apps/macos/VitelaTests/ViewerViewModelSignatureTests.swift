import AppKit
import XCTest
@testable import Vitela

final class ViewerViewModelSignatureTests: XCTestCase {
    private final class MemorySignatures: SignatureStore {
        var stored: Data?
        var saveSucceeds = true
        func load() -> Data? { stored }
        func save(_ png: Data) -> Bool {
            guard saveSucceeds else { return false }
            stored = png
            return true
        }
        func delete() { stored = nil }
    }

    private func model(_ signatures: MemorySignatures) -> ViewerViewModel {
        let model = ViewerViewModel(store: ViewerStore(client: FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])), signatures: signatures)
        model.store.open(bytes: Data([1]))
        return model
    }

    private let stroke = [[PadPoint(x: 10, y: 10), PadPoint(x: 80, y: 40), PadPoint(x: 150, y: 15)]]

    func testThePadsStrokesBecomeATransparentPngThatIsArmedAndRemembered() throws {
        let signatures = MemorySignatures()
        let model = model(signatures)
        model.drawSignature()
        waitUntil({ model.signatureSheet == .pad }, "no saved signature, so the pad opens")

        model.useDrawnSignature(stroke, remember: true)

        waitUntil({ model.store.armedStampImage != nil }, "the signature was never armed")
        let png = try XCTUnwrap(model.store.armedStampImage)
        XCTAssertTrue(ViewerStore.isSupportedStampImage(png))
        let image = try XCTUnwrap(NSBitmapImageRep(data: png))
        XCTAssertTrue(image.hasAlpha)
        XCTAssertEqual(image.pixelsWide, 143, "cropped to the ink (140) plus half a stroke each side (3)")
        XCTAssertEqual(signatures.stored, png)
    }

    func testARememberedSignatureIsOfferedFirstAndCanBeDeleted() {
        let signatures = MemorySignatures()
        signatures.stored = Data([137, 80, 78, 71, 13, 10, 26, 10])
        let model = model(signatures)

        model.drawSignature()
        waitUntil({ model.signatureSheet == .saved(signatures.stored!) }, "the remembered signature was not offered")
        model.chooseSavedSignature(.delete)

        waitUntil({ signatures.stored == nil }, "the signature was not forgotten")
        XCTAssertEqual(model.store.editStatus, "Your saved signature was deleted from this Mac.")
    }

    func testUsingTheRememberedSignatureArmsIt() {
        let signatures = MemorySignatures()
        let png = Data([137, 80, 78, 71, 13, 10, 26, 10, 1])
        signatures.stored = png
        let model = model(signatures)
        model.drawSignature()
        waitUntil({ model.signatureSheet != nil }, "no sheet")

        model.chooseSavedSignature(.use)

        XCTAssertEqual(model.store.armedStampImage, png)
    }

    func testAFailedRememberStillArmsTheSignature() {
        let signatures = MemorySignatures()
        signatures.saveSucceeds = false
        let model = model(signatures)

        model.useDrawnSignature(stroke, remember: true)

        waitUntil({ model.store.armedStampImage != nil }, "the signature was never armed")
        XCTAssertEqual(model.store.editStatus, "Your signature could not be remembered. Click a page to place it.")
    }

    func testTheFileStoreWritesReadsAndForgets() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("vitela-signature-\(UUID().uuidString)", isDirectory: true)
            .appendingPathComponent("signature.png")
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = FileSignatureStore(url: url)

        XCTAssertNil(store.load())
        XCTAssertTrue(store.save(Data([1, 2, 3])))
        XCTAssertEqual(store.load(), Data([1, 2, 3]))
        store.delete()
        XCTAssertNil(store.load())
    }
}
