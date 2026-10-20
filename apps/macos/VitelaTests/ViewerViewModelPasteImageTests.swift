import AppKit
import XCTest
@testable import Vitela

/// An image from outside placed as a stamp: ⌘V reads only a bitmap from the
/// clipboard, and a file dropped on a page stamps an image there.
final class ViewerViewModelPasteImageTests: XCTestCase {
    private func model() -> (FakePdfCoreClient, ViewerViewModel) {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client), sampleLoader: { Data([1]) })
        model.openSample()
        waitUntil({ model.store.state == .loaded }, "the sample never opened")
        return (client, model)
    }

    private func pasteboard() -> NSPasteboard {
        let board = NSPasteboard(name: NSPasteboard.Name("vitela-test-\(UUID().uuidString)"))
        board.clearContents()
        addTeardownBlock { board.releaseGlobally() }
        return board
    }

    private func bitmap() throws -> NSBitmapImageRep {
        try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 2, pixelsHigh: 2, bitsPerSample: 8, samplesPerPixel: 3,
            hasAlpha: false, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
    }

    func testACopiedBitmapIsReEncodedAsPngAndStamped() throws {
        let (client, model) = model()
        let board = pasteboard()
        board.setData(try XCTUnwrap(try bitmap().tiffRepresentation), forType: .tiff)

        model.pasteImageStamp(from: board)

        XCTAssertEqual(client.stampInserts.count, 1)
        let stamped = try XCTUnwrap(model.store.stampImages.values.first)
        XCTAssertTrue(ViewerStore.isSupportedStampImage(stamped), "the TIFF reached the core as a PNG")
    }

    func testCopiedTextIsNeverReadEvenWhenItIsAUrl() {
        let (client, model) = model()
        let board = pasteboard()
        board.setString("https://example.com/logo.png", forType: .string)

        model.pasteImageStamp(from: board)

        XCTAssertTrue(client.stampInserts.isEmpty)
        XCTAssertEqual(model.store.editStatus, ViewerStore.clipboardHasNoImage)
    }

    func testPasteIsLeftToATextFieldAndToOrganize() throws {
        let (_, model) = model()
        let event = try XCTUnwrap(NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: .command, timestamp: 0, windowNumber: 0,
            context: nil, characters: "v", charactersIgnoringModifiers: "v", isARepeat: false, keyCode: 9
        ))
        XCTAssertTrue(model.takesPaste(event))

        model.isOrganizing = true
        XCTAssertFalse(model.takesPaste(event))
        model.isOrganizing = false

        let shifted = try XCTUnwrap(NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: [.command, .shift], timestamp: 0, windowNumber: 0,
            context: nil, characters: "V", charactersIgnoringModifiers: "v", isARepeat: false, keyCode: 9
        ))
        XCTAssertFalse(model.takesPaste(shifted))
    }

    func testAnImageDroppedOnAPageIsStampedWhereItLanded() throws {
        let (client, model) = model()
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("drop-\(UUID().uuidString).png")
        try XCTUnwrap(try bitmap().representation(using: .png, properties: [:])).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }

        // At zoom 1 a drop 100 points in and 92 down is PDF (100, 700).
        model.dropFiles([url], onPage: 0, at: CGPoint(x: 100, y: 92), zoom: 1)

        waitUntil({ client.stampInserts.count == 1 }, "the dropped image was never stamped")
        XCTAssertEqual(client.stampInserts.first?.rect, PdfRect(x: 100, y: 650, width: 100, height: 50))
    }

    func testAnImageDroppedOnTheWindowSaysToAimAtAPage() throws {
        let (_, model) = model()
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("drop-\(UUID().uuidString).png")
        try XCTUnwrap(try bitmap().representation(using: .png, properties: [:])).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }

        model.openExternalFiles([url])

        waitUntil({ model.notice == "Drop the image onto a page to place it as a stamp." }, "no notice for the image")
    }
}
