import XCTest
@testable import Vitela

final class ViewerViewModelSaveTests: XCTestCase {
    private var directory: URL!

    override func setUpWithError() throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("vitela-save-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: directory)
    }

    /// Records what each prompt was asked and answers from the test's script.
    private final class Script {
        var destination: URL?
        var acceptSignatureLoss = false
        var unsavedChoice = SavePrompts.UnsavedChoice.cancel
        var suggestedNames: [String] = []
        var signatureQuestions = 0
        var unsavedQuestions = 0

        var prompts: SavePrompts {
            SavePrompts(
                chooseDestination: { name in self.suggestedNames.append(name); return self.destination },
                confirmSignatureLoss: { self.signatureQuestions += 1; return self.acceptSignatureLoss },
                resolveUnsavedChanges: { self.unsavedQuestions += 1; return self.unsavedChoice }
            )
        }
    }

    private func makeModel(_ client: FakePdfCoreClient, _ script: Script) -> ViewerViewModel {
        let model = ViewerViewModel(store: ViewerStore(client: client), prompts: script.prompts)
        model.store.open(bytes: Data([1]))
        return model
    }

    private func saveAndWait(_ model: ViewerViewModel) -> Bool? {
        var outcome: Bool?
        model.saveAs { outcome = $0 }
        waitUntil({ outcome != nil }, "the save never finished")
        return outcome
    }

    func testSaveAsWritesTheCoreBytesAndClearsUnsavedChanges() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let script = Script()
        script.destination = directory.appendingPathComponent("out.pdf")
        let model = makeModel(client, script)
        model.store.setDocumentProperty(.title, to: "Saved")

        XCTAssertEqual(saveAndWait(model), true)

        XCTAssertEqual(try Data(contentsOf: script.destination!), client.savedBytes)
        XCTAssertFalse(model.store.hasUnsavedChanges)
        XCTAssertFalse(model.isSaving)
        XCTAssertEqual(script.signatureQuestions, 0, "an unsigned file must not ask about signatures")
        XCTAssertEqual(client.saveAcknowledgements, [false])
    }

    func testCancellingTheSavePanelWritesNothing() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let script = Script()
        let model = makeModel(client, script)

        XCTAssertEqual(saveAndWait(model), false)

        XCTAssertTrue(client.saveAcknowledgements.isEmpty)
        XCTAssertEqual(model.store.editStatus, "Save cancelled.")
        XCTAssertEqual(script.suggestedNames, ["document.pdf"])
    }

    func testASignedDocumentIsSavedOnlyAfterTheUserAcceptsBreakingTheSignature() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.invalidatesSignatures = true
        let script = Script()
        script.destination = directory.appendingPathComponent("signed.pdf")
        let model = makeModel(client, script)

        XCTAssertEqual(saveAndWait(model), false)
        XCTAssertEqual(script.signatureQuestions, 1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: script.destination!.path))
        XCTAssertEqual(model.store.editStatus, "Save cancelled.")

        script.acceptSignatureLoss = true
        XCTAssertEqual(saveAndWait(model), true)
        XCTAssertEqual(client.saveAcknowledgements, [true])
        XCTAssertTrue(FileManager.default.fileExists(atPath: script.destination!.path))
    }

    func testACoreSaveFailureIsReportedAndLeavesChangesUnsaved() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.saveError = EditFailure.failed("Disk full.")
        let script = Script()
        script.destination = directory.appendingPathComponent("out.pdf")
        let model = makeModel(client, script)
        model.store.setDocumentProperty(.title, to: "Edited")

        XCTAssertEqual(saveAndWait(model), false)

        XCTAssertEqual(model.store.editStatus, "Disk full.")
        XCTAssertTrue(model.store.hasUnsavedChanges)
        XCTAssertFalse(FileManager.default.fileExists(atPath: script.destination!.path))
    }

    func testUnsavedChangesAskBeforeTheyWouldBeLost() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let script = Script()
        let model = makeModel(client, script)

        var answers: [Bool] = []
        model.resolveUnsavedChanges { answers.append($0) }
        XCTAssertEqual(answers, [true], "a clean document proceeds without asking")
        XCTAssertEqual(script.unsavedQuestions, 0)

        model.store.setDocumentProperty(.title, to: "Edited")
        script.unsavedChoice = .cancel
        model.resolveUnsavedChanges { answers.append($0) }
        script.unsavedChoice = .discard
        model.resolveUnsavedChanges { answers.append($0) }
        XCTAssertEqual(answers, [true, false, true])
    }

    func testChoosingSaveProceedsOnlyOnceTheFileIsWritten() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let script = Script()
        script.unsavedChoice = .save
        script.destination = directory.appendingPathComponent("kept.pdf")
        let model = makeModel(client, script)
        model.store.setDocumentProperty(.title, to: "Edited")

        var answer: Bool?
        model.resolveUnsavedChanges { answer = $0 }
        waitUntil({ answer != nil }, "the save never answered")

        XCTAssertEqual(answer, true)
        XCTAssertTrue(FileManager.default.fileExists(atPath: script.destination!.path))
        XCTAssertFalse(model.store.hasUnsavedChanges)
    }
}
