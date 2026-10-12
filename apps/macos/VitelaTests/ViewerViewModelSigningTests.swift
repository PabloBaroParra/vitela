import XCTest
@testable import Vitela

final class ViewerViewModelSigningTests: XCTestCase {
    private var directory: URL!

    override func setUpWithError() throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("vitela-sign-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: directory)
    }

    private final class Script {
        var certificate: URL?
        var passwords: [String] = []
        var retries: [Bool] = []
        var identityIndex: Int? = 0
        var destination: URL?
        var suggested: [String] = []
    }

    private func model(_ client: FakePdfCoreClient, _ script: Script) -> ViewerViewModel {
        let signing = SigningPrompts(
            chooseCertificate: { script.certificate },
            askPassword: { retry in
                script.retries.append(retry)
                return script.passwords.isEmpty ? nil : script.passwords.removeFirst()
            },
            chooseIdentity: { identities in script.identityIndex.map { identities[$0] } }
        )
        let save = SavePrompts(
            chooseDestination: { name in script.suggested.append(name); return script.destination },
            confirmSignatureLoss: { false },
            resolveUnsavedChanges: { .cancel }
        )
        let model = ViewerViewModel(store: ViewerStore(client: client), prompts: save, signingPrompts: signing, sampleLoader: { Data([1]) })
        model.openSample()
        waitUntil({ model.store.state == .loaded }, "the sample never opened")
        return model
    }

    private func certificateFile() throws -> URL {
        let url = directory.appendingPathComponent("me.p12")
        try Data([1, 2, 3]).write(to: url)
        return url
    }

    func testSigningWritesTheSignedCopyAndReopensIt() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let script = Script()
        script.certificate = try certificateFile()
        script.passwords = ["wrong", "secret"]
        script.destination = directory.appendingPathComponent("signed.pdf")
        let model = model(client, script)

        model.signWithCertificateFile()

        waitUntil({ model.store.editStatus == "Signed PDF saved and reopened." }, "the signed file never reopened")
        XCTAssertEqual(script.retries, [false, true], "a wrong password asks again, saying so")
        XCTAssertEqual(client.signedAs, ["id-1"])
        XCTAssertEqual(try Data(contentsOf: script.destination!), client.signedBytes)
        XCTAssertEqual(client.reopenedSigned, [client.signedBytes])
        XCTAssertEqual(script.suggested, ["vitela-sample-signed.pdf"])
        XCTAssertEqual(model.documentName, "signed.pdf")
        XCTAssertFalse(model.isSaving)
    }

    func testARefusingDocumentNeverAsksForACertificate() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.signingRefusalText = "This document does not permit adding signature fields."
        let script = Script()
        script.certificate = URL(fileURLWithPath: "/never/asked.p12")
        let model = model(client, script)

        model.signWithCertificateFile()

        XCTAssertEqual(model.store.editStatus, "This document does not permit adding signature fields.")
        XCTAssertTrue(script.retries.isEmpty, "no password was asked for")
        XCTAssertTrue(client.unlockAttempts.isEmpty)
    }

    func testASourceWithoutIdentitiesIsReported() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.signingIdentities = []
        let script = Script()
        script.certificate = try certificateFile()
        script.passwords = ["secret"]
        let model = model(client, script)

        model.signWithCertificateFile()

        waitUntil({ model.store.editStatus == "This certificate source has no usable signing identities." }, "never reported")
        XCTAssertTrue(client.signedAs.isEmpty)
    }

    func testCancellingAnyStepSignsNothing() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let script = Script()
        script.certificate = try certificateFile()
        script.passwords = ["secret"]
        script.identityIndex = nil
        let model = model(client, script)

        model.signWithCertificateFile()

        waitUntil({ model.store.editStatus == "Signing cancelled." }, "the cancel was never reported")
        XCTAssertTrue(client.signedAs.isEmpty)
    }

    func testTheSignedStateIsReadOnOpen() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.sourceIsSigned = true
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        XCTAssertTrue(store.isSigned)
        XCTAssertNil(store.signingRefusal)
    }
}
