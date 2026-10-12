import XCTest
@testable import Vitela

final class ViewerViewModelImportTests: XCTestCase {
    private var directory: URL!
    private let letter = PageDimensions(width: 612, height: 792)
    private let a5 = PageDimensions(width: 420, height: 595)

    override func setUpWithError() throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("vitela-import-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: directory)
    }

    /// Records what each prompt was asked and answers from the test's script.
    private final class Script {
        var picked: [URL] = []
        var passwords: [String?] = []
        var acceptWarnings = true
        var passwordQuestions: [(String, Bool)] = []
        var warningsShown: [[String]] = []

        var prompts: ImportPrompts {
            ImportPrompts(
                choosePdfs: { self.picked },
                askPassword: { name, retry in
                    self.passwordQuestions.append((name, retry))
                    return self.passwords.isEmpty ? nil : self.passwords.removeFirst()
                },
                confirmWarnings: { self.warningsShown.append($0); return self.acceptWarnings }
            )
        }
    }

    private func source(_ name: String, _ source: FakeImportSource, in client: FakePdfCoreClient) throws -> URL {
        let bytes = Data("%PDF-\(name)".utf8)
        client.importSources[bytes] = source
        let url = directory.appendingPathComponent(name)
        try bytes.write(to: url)
        return url
    }

    private func makeModel(_ client: FakePdfCoreClient, _ script: Script) -> ViewerViewModel {
        let model = ViewerViewModel(store: ViewerStore(client: client), importPrompts: script.prompts)
        model.store.open(bytes: Data([1]))
        model.isOrganizing = true
        return model
    }

    func testEveryPickedPdfIsAppendedAsOneUndoStep() throws {
        let client = FakePdfCoreClient(pages: [letter])
        let script = Script()
        script.picked = [
            try source("a.pdf", FakeImportSource(pages: [a5, a5]), in: client),
            try source("b.pdf", FakeImportSource(pages: [letter]), in: client),
        ]
        let model = makeModel(client, script)

        model.importPdfs()

        waitUntil({ model.store.pageSlots.count == 4 && !model.store.pageWorkInFlight }, "the import never landed")
        XCTAssertEqual(model.store.pageSlots.map(\.dimensions), [letter, a5, a5, letter])
        XCTAssertEqual(model.store.editStatus, "Imported 3 pages from 2 PDFs. Changes are pending save.")
        XCTAssertEqual(client.historyCursor, 1, "the whole pick is one undo step")
        XCTAssertFalse(model.isImporting)
    }

    func testARefusingDocumentIsReportedBeforeAnyFileIsPicked() {
        let client = FakePdfCoreClient(pages: [letter])
        client.importRefusalText = "This document does not allow adding pages."
        var asked = false
        let model = ViewerViewModel(
            store: ViewerStore(client: client),
            importPrompts: ImportPrompts(choosePdfs: { asked = true; return [] }, askPassword: { _, _ in nil }, confirmWarnings: { _ in true })
        )
        model.store.open(bytes: Data([1]))
        model.isOrganizing = true

        model.importPdfs()

        XCTAssertFalse(asked)
        XCTAssertEqual(model.store.editStatus, "This document does not allow adding pages.")
    }

    func testALockedSourceAsksForItsPasswordUntilItOpens() throws {
        let client = FakePdfCoreClient(pages: [letter])
        let script = Script()
        script.picked = [try source("locked.pdf", FakeImportSource(pages: [a5], password: "right"), in: client)]
        script.passwords = ["wrong", "right"]
        let model = makeModel(client, script)

        model.importPdfs()

        waitUntil({ model.store.pageSlots.count == 2 }, "the locked source never imported")
        XCTAssertEqual(script.passwordQuestions.map(\.0), ["locked.pdf", "locked.pdf"])
        XCTAssertEqual(script.passwordQuestions.map(\.1), [false, true], "the second ask says the first was wrong")
        XCTAssertEqual(client.preparedPasswords, [nil, "wrong", "right"])
    }

    func testCancellingAPasswordCancelsTheWholePickAndAddsNothing() throws {
        let client = FakePdfCoreClient(pages: [letter])
        let script = Script()
        script.picked = [
            try source("a.pdf", FakeImportSource(pages: [a5]), in: client),
            try source("locked.pdf", FakeImportSource(pages: [a5], password: "secret"), in: client),
        ]
        let model = makeModel(client, script)

        model.importPdfs()

        waitUntil({ !model.isImporting }, "the import never ended")
        XCTAssertEqual(model.store.editStatus, ViewerViewModel.importCancelled)
        XCTAssertEqual(model.store.pageSlots.count, 1)
        XCTAssertEqual(client.historyCursor, 0)
    }

    func testWarningsAreAskedAboutBeforeAnythingChanges() throws {
        let client = FakePdfCoreClient(pages: [letter])
        let script = Script()
        script.acceptWarnings = false
        script.picked = [try source("a.pdf", FakeImportSource(pages: [a5], warnings: ["links will not work"]), in: client)]
        let model = makeModel(client, script)

        model.importPdfs()

        waitUntil({ !model.isImporting }, "the import never ended")
        XCTAssertEqual(script.warningsShown, [["a.pdf: links will not work"]])
        XCTAssertEqual(model.store.editStatus, ViewerViewModel.importCancelled)
        XCTAssertEqual(model.store.pageSlots.count, 1)
    }

    func testASourceTheCoreRefusesNamesTheFile() throws {
        let client = FakePdfCoreClient(pages: [letter])
        let script = Script()
        let junk = directory.appendingPathComponent("junk.pdf")
        try Data("not a pdf".utf8).write(to: junk)
        script.picked = [junk]
        let model = makeModel(client, script)

        model.importPdfs()

        waitUntil({ !model.isImporting }, "the import never ended")
        XCTAssertEqual(model.store.editStatus, "Could not import junk.pdf: Not a PDF.")
    }

    func testAnEmptyPickIsACancel() {
        let client = FakePdfCoreClient(pages: [letter])
        let model = makeModel(client, Script())

        model.importPdfs()

        XCTAssertEqual(model.store.editStatus, ViewerViewModel.importCancelled)
        XCTAssertFalse(model.isImporting)
    }

    func testUndoRemovesTheImportedPages() throws {
        let client = FakePdfCoreClient(pages: [letter])
        let script = Script()
        script.picked = [try source("a.pdf", FakeImportSource(pages: [a5, a5]), in: client)]
        let model = makeModel(client, script)
        model.importPdfs()
        waitUntil({ model.store.pageSlots.count == 3 && !model.store.pageWorkInFlight }, "the import never landed")

        model.undo()

        waitUntil({ model.store.pageSlots.count == 1 }, "undo never removed the imported pages")
    }
}
