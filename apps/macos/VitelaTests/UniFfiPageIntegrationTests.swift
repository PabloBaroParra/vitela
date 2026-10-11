import XCTest
@testable import Vitela

/// Page-structure edits and imports against the real core: what the reader
/// sees after the preview is re-derived, and what the saved file holds.
final class UniFfiPageIntegrationTests: XCTestCase {
    /// A page edit only reaches the pixels once the render side is
    /// re-derived; this checks the layout the reader shows, and the saved file.
    func testARotatedPageIsShownAndSavedTurned() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let before = try XCTUnwrap(store.pageSlots.first?.dimensions)

        let request = try XCTUnwrap(store.beginPageEdit(.rotate(page: 0, degrees: 90)))
        let outcome = store.pageWorkResult(for: request)
        store.applyPageWork(outcome, for: request)

        XCTAssertEqual(store.editStatus, "Page rotated. Changes are pending save.")
        // Sizes come with the turn applied, and the turn itself is reported
        // so overlays can be placed on the drawn page.
        let turned = PageDimensions(width: before.height, height: before.width, rotation: .clockwise90)
        XCTAssertEqual(store.pageSlots.first?.dimensions, turned)
        let render = try XCTUnwrap(store.beginRender(page: 0))
        let image = try store.renderResult(for: render).get()
        XCTAssertGreaterThan(image.width, image.height, "the preview must draw the turned page")

        let save = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        let reopened = ViewerStore(client: UniFfiPdfCoreClient())
        reopened.open(bytes: try store.saveResult(for: save).get())
        XCTAssertEqual(reopened.pageSlots.first?.dimensions, turned)

        store.undo()
        let refresh = try XCTUnwrap(store.beginPageRefresh())
        store.applyPageWork(store.pageWorkResult(for: refresh), for: refresh)
        XCTAssertEqual(store.pageSlots.first?.dimensions, before)
    } }

    func testADeletedPageIsGoneFromThePreviewAndTheSavedFile() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let count = store.pageSlots.count
        try XCTSkipIf(count < 2, "the sample needs two pages to delete one")

        let request = try XCTUnwrap(store.beginPageEdit(.remove(page: 0)))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
        XCTAssertEqual(store.pageSlots.count, count - 1)

        let save = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        let reopened = ViewerStore(client: UniFfiPdfCoreClient())
        reopened.open(bytes: try store.saveResult(for: save).get())
        XCTAssertEqual(reopened.pageSlots.count, count - 1)
    } }

    func testAnInsertedBlankPageIsA4InThePreviewAndTheSavedFile() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        let count = store.pageSlots.count

        let request = try XCTUnwrap(store.beginPageEdit(.insertBlank(at: 0, orientation: .landscape)))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertEqual(store.pageSlots.count, count + 1)
        let blank = try XCTUnwrap(store.pageSlots.first?.dimensions)
        XCTAssertEqual(blank.width, 842, accuracy: 1)
        XCTAssertEqual(blank.height, 595, accuracy: 1)

        let save = try XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        let reopened = ViewerStore(client: UniFfiPdfCoreClient())
        reopened.open(bytes: try store.saveResult(for: save).get())
        XCTAssertEqual(reopened.pageSlots.count, count + 1)
    } }

    func testAMovedPageKeepsItsSizeAtItsNewPosition() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        // A blank landscape page first gives the move something to tell apart.
        let insert = try XCTUnwrap(store.beginPageEdit(.insertBlank(at: 0, orientation: .landscape)))
        store.applyPageWork(store.pageWorkResult(for: insert), for: insert)
        let last = store.pageSlots.count - 1
        let blank = store.pageSlots[0].dimensions

        let move = try XCTUnwrap(store.beginPageEdit(.move(from: 0, to: last)))
        store.applyPageWork(store.pageWorkResult(for: move), for: move)

        XCTAssertEqual(store.editStatus, "Page moved. Changes are pending save.")
        XCTAssertEqual(store.pageSlots[last].dimensions, blank)
        XCTAssertNotEqual(store.pageSlots[0].dimensions, blank)
    } }

    func testImportingAPdfAppendsAllItsPagesAsOneStep() throws { try offMain {
        let client = UniFfiPdfCoreClient()
        let store = ViewerStore(client: client)
        store.open(bytes: try Self.sampleBytes())
        let count = store.pageSlots.count
        XCTAssertNil(store.importRefusal())

        let source = try store.prepareImport(bytes: try Self.sampleBytes(), password: nil).get()
        XCTAssertEqual(source.pageCount, count)
        let request = try XCTUnwrap(store.beginImport([source]))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertEqual(store.pageSlots.count, 2 * count)
        XCTAssertEqual(store.editStatus, "Imported \(count) \(count == 1 ? "page" : "pages") from 1 PDF. Changes are pending save.")
        store.undo()
        XCTAssertFalse(store.canUndo, "the import is a single history step")
    } }

    func testAnEncryptedSourceNeedsItsPassword() throws { try offMain {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "aes_128_user_and_owner", withExtension: "pdf"))
        let bytes = try Data(contentsOf: url)
        let store = ViewerStore(client: UniFfiPdfCoreClient())

        guard case let .failure(error) = store.prepareImport(bytes: bytes, password: nil) else {
            return XCTFail("an encrypted source must not prepare without its password")
        }
        XCTAssertTrue([ViewerFailure.passwordRequired, .wrongPassword].contains(error as? ViewerFailure))
        if case let .failure(error) = store.prepareImport(bytes: bytes, password: "user-aes-pass") {
            // The core may still refuse it for its own permissions; that is
            // a refusal, not a password question.
            XCTAssertNotNil(error as? EditFailure)
        }
    } }
}
