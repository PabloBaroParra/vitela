import XCTest
@testable import Vitela

final class ViewerViewModelCompressTests: XCTestCase {
    private var directory: URL!

    override func setUpWithError() throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("vitela-compress-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: directory)
    }

    private final class Script {
        var destination: URL?
        var acceptSignatureLoss = false
        var destinationQuestions = 0
        var signatureQuestions = 0

        var prompts: SavePrompts {
            SavePrompts(
                chooseDestination: { _ in self.destinationQuestions += 1; return self.destination },
                confirmSignatureLoss: { self.signatureQuestions += 1; return self.acceptSignatureLoss },
                resolveUnsavedChanges: { .cancel }
            )
        }
    }

    private func makeModel(_ client: FakePdfCoreClient, _ script: Script) -> ViewerViewModel {
        let model = ViewerViewModel(store: ViewerStore(client: client), prompts: script.prompts)
        model.store.open(bytes: Data([1]))
        return model
    }

    private func client() -> FakePdfCoreClient { FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)]) }

    func testASmallerCopyIsWrittenAndTheOpenDocumentStaysAsItWas() throws {
        let client = client()
        let script = Script()
        script.destination = directory.appendingPathComponent("small.pdf")
        let model = makeModel(client, script)
        model.store.setDocumentProperty(.title, to: "Pending")
        model.requestCompress()
        XCTAssertTrue(model.isCompressPresented)

        model.compress(.small)

        waitUntil({ !model.isSaving }, "the compression never finished")
        XCTAssertEqual(try Data(contentsOf: try XCTUnwrap(script.destination)), client.compression.bytes)
        XCTAssertEqual(client.compressCalls.map(\.preset), [.small])
        XCTAssertEqual(model.store.editStatus, "Compressed PDF written to \(script.destination!.path) (2.0 MB → 1.0 MB, 49% smaller).")
        XCTAssertTrue(model.store.hasUnsavedChanges, "compressing is not saving: the edit stays pending")
        XCTAssertTrue(model.store.canUndo)
    }

    func testNothingSmallerMeansNoDestinationIsAskedFor() {
        let client = client()
        client.compression = CompressionResult(bytes: Data(), beforeBytes: 5_000, afterBytes: 5_000, savedBytes: 0, reduced: false, refusals: [])
        let script = Script()
        let model = makeModel(client, script)

        model.compress(.lossless)

        waitUntil({ !model.isSaving }, "the compression never finished")
        XCTAssertEqual(script.destinationQuestions, 0)
        XCTAssertTrue(model.store.editStatus.hasPrefix("Nothing to gain"))
    }

    func testARefusingDocumentNeverShowsTheDialog() {
        let client = client()
        client.compressRefusalText = "This document's password protection does not allow it to be rewritten, so it cannot be compressed."
        let model = makeModel(client, Script())

        model.requestCompress()

        XCTAssertFalse(model.isCompressPresented)
        XCTAssertEqual(model.store.editStatus, client.compressRefusalText)
    }

    func testASignedDocumentAsksFirstAndDecliningCompressesNothing() {
        let client = client()
        client.compressInvalidatesSignatures = true
        let script = Script()
        let model = makeModel(client, script)

        model.compress(.balanced)

        waitUntil({ !model.isSaving }, "the compression never finished")
        XCTAssertEqual(script.signatureQuestions, 1)
        XCTAssertTrue(client.compressCalls.isEmpty)
        XCTAssertEqual(model.store.editStatus, "Compression cancelled.")
    }

    func testAcceptingTheSignatureLossIsPassedToTheCore() {
        let client = client()
        client.compressInvalidatesSignatures = true
        let script = Script()
        script.acceptSignatureLoss = true
        let model = makeModel(client, script)

        model.compress(.balanced)

        waitUntil({ !model.isSaving }, "the compression never finished")
        XCTAssertEqual(client.compressCalls.map(\.acknowledged), [true])
        XCTAssertEqual(model.store.editStatus, "Compression cancelled. No file was written.")
    }
}
