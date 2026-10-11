import XCTest
@testable import Vitela

final class ViewerViewModelProtectTests: XCTestCase {
    private var directory: URL!

    override func setUpWithError() throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("vitela-protect-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: directory)
    }

    private final class Script {
        var destination: URL?
        var acceptSignatureLoss = false
        var signatureQuestions = 0

        var prompts: SavePrompts {
            SavePrompts(
                chooseDestination: { _ in self.destination },
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

    func testTheProtectedFileIsWrittenAndBecomesTheOpenDocument() throws {
        let client = client()
        let script = Script()
        script.destination = directory.appendingPathComponent("locked.pdf")
        let model = makeModel(client, script)
        model.requestProtect()
        XCTAssertTrue(model.isProtectPresented)

        XCTAssertNil(model.protect(openPassword: "open", permissionsPassword: "owner"))
        XCTAssertFalse(model.isProtectPresented)

        waitUntil({ model.store.editStatus == "Protected PDF saved and reopened." }, "the protected file never reopened")
        XCTAssertEqual(try Data(contentsOf: try XCTUnwrap(script.destination)), client.protectedBytes)
        XCTAssertEqual(model.documentName, "locked.pdf")
        XCTAssertFalse(model.isSaving)
    }

    func testAPasswordProblemKeepsTheSheetOpenAndProtectsNothing() {
        let client = client()
        let model = makeModel(client, Script())
        model.requestProtect()

        XCTAssertEqual(model.protect(openPassword: "x", permissionsPassword: ""), "Enter the permissions password.")

        XCTAssertTrue(model.isProtectPresented)
        XCTAssertTrue(client.protectCalls.isEmpty)
    }

    func testASignedDocumentAsksFirstAndDecliningWritesNothing() {
        let client = client()
        client.protectInvalidatesSignatures = true
        let script = Script()
        script.destination = directory.appendingPathComponent("locked.pdf")
        let model = makeModel(client, script)

        XCTAssertNil(model.protect(openPassword: "open", permissionsPassword: "owner"))

        waitUntil({ !model.isSaving }, "the protection never ended")
        XCTAssertEqual(script.signatureQuestions, 1)
        XCTAssertTrue(client.protectCalls.isEmpty)
        XCTAssertEqual(model.store.editStatus, "Protection cancelled.")
    }

    func testCancellingTheDestinationWritesNothing() {
        let client = client()
        let model = makeModel(client, Script())

        XCTAssertNil(model.protect(openPassword: "open", permissionsPassword: "owner"))

        waitUntil({ !model.isSaving }, "the protection never ended")
        XCTAssertEqual(model.store.editStatus, "Protection cancelled.")
        XCTAssertTrue(client.protectCalls.isEmpty)
    }

    func testAFailedProtectionKeepsTheOpenDocument() {
        let client = client()
        client.protectError = EditFailure.notPermitted("This document's encryption cannot be changed.")
        let script = Script()
        script.destination = directory.appendingPathComponent("locked.pdf")
        let model = makeModel(client, script)

        XCTAssertNil(model.protect(openPassword: "open", permissionsPassword: "owner"))

        waitUntil({ !model.isSaving }, "the protection never ended")
        XCTAssertEqual(model.store.editStatus, "This document's encryption cannot be changed.")
        XCTAssertEqual(client.openedBytes.count, 1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: script.destination!.path))
    }
}
