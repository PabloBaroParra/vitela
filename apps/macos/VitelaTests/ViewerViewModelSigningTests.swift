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

/// A Keychain stand-in: fixed identities, and a record of what it was asked.
private final class FakeKeychain: NativeSigningStore {
    var listed: [NativeSigningIdentity]
    var failure: Error?

    init(_ listed: [NativeSigningIdentity], failure: Error? = nil) {
        self.listed = listed
        self.failure = failure
    }

    func identities() throws -> [NativeSigningIdentity] {
        if let failure { throw failure }
        return listed
    }

    func signDigest(identityId: String, digest: Data, algorithm: NativeSigningAlgorithm) throws -> Data { Data() }
}

extension ViewerViewModelSigningTests {
    private func keychainModel(_ client: FakePdfCoreClient, keychain: FakeKeychain, destination: URL?) -> ViewerViewModel {
        var signing = SigningPrompts.modal
        signing.chooseIdentity = { $0.last }
        signing.openKeychain = { keychain }
        let save = SavePrompts(chooseDestination: { _ in destination }, confirmSignatureLoss: { false }, resolveUnsavedChanges: { .cancel })
        let model = ViewerViewModel(store: ViewerStore(client: client), prompts: save, signingPrompts: signing, sampleLoader: { Data([1]) })
        model.openSample()
        waitUntil({ model.store.state == .loaded }, "the sample never opened")
        return model
    }

    func testAKeychainIdentitySignsLikeACertificateFile() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let keychain = FakeKeychain([
            NativeSigningIdentity(id: "A1", displayName: "Ana", certificateChainDer: [Data([1])], algorithm: .rsaSha256),
            NativeSigningIdentity(id: "B2", displayName: "Bo", certificateChainDer: [Data([2])], algorithm: .ecdsaSha256),
        ])
        let destination = directory.appendingPathComponent("signed.pdf")
        let model = keychainModel(client, keychain: keychain, destination: destination)

        model.signWithKeychainIdentity()

        waitUntil({ client.signedAs == ["B2"] && !model.isSaving }, "the Keychain identity never signed")
        XCTAssertEqual(try Data(contentsOf: destination), client.signedBytes)
    }

    func testAnEmptyKeychainSaysWhereToGetAnIdentity() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = keychainModel(client, keychain: FakeKeychain([]), destination: nil)

        model.signWithKeychainIdentity()

        waitUntil({ model.store.editStatus.hasPrefix("No signing identity") }, "the empty Keychain was never reported")
        XCTAssertTrue(client.signedAs.isEmpty)
    }

    func testAKeychainThatCannotBeReadSaysWhy() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let failing = FakeKeychain([], failure: KeychainSigningFailure(message: "The Keychain could not be read (locked)."))
        let model = keychainModel(client, keychain: failing, destination: nil)

        model.signWithKeychainIdentity()

        waitUntil({ model.store.editStatus == "The Keychain could not be read (locked)." }, "the failure was never reported")
    }

    func testOnlyRsaAndP256KeysCanSign() {
        XCTAssertEqual(KeychainSigningStore.algorithm(keyType: kSecAttrKeyTypeRSA as String, keySizeInBits: 2048), .rsaSha256)
        XCTAssertEqual(KeychainSigningStore.algorithm(keyType: kSecAttrKeyTypeECSECPrimeRandom as String, keySizeInBits: 256), .ecdsaSha256)
        XCTAssertNil(KeychainSigningStore.algorithm(keyType: kSecAttrKeyTypeECSECPrimeRandom as String, keySizeInBits: 384))
        XCTAssertNil(KeychainSigningStore.algorithm(keyType: nil, keySizeInBits: nil))
    }
}

extension ViewerViewModelSigningTests {
    private func tokenModel(_ client: FakePdfCoreClient, module: URL?, pin: String?, destination: URL?) -> ViewerViewModel {
        var signing = SigningPrompts.modal
        signing.chooseIdentity = { $0.first }
        signing.chooseTokenModule = { module }
        signing.askTokenPin = { pin }
        let save = SavePrompts(chooseDestination: { _ in destination }, confirmSignatureLoss: { false }, resolveUnsavedChanges: { .cancel })
        let model = ViewerViewModel(store: ViewerStore(client: client), prompts: save, signingPrompts: signing, sampleLoader: { Data([1]) })
        model.openSample()
        waitUntil({ model.store.state == .loaded }, "the sample never opened")
        return model
    }

    func testATokenOpensWithItsPinAndSignsLikeACertificateFile() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let module = URL(fileURLWithPath: "/Library/OpenSC/lib/opensc-pkcs11.so")
        let destination = directory.appendingPathComponent("signed.pdf")
        let model = tokenModel(client, module: module, pin: "1234", destination: destination)

        model.signWithToken()

        waitUntil({ client.signedAs == ["id-1"] && !model.isSaving }, "the token never signed")
        XCTAssertEqual(client.tokenOpens.map(\.module), [module.path])
        XCTAssertEqual(client.tokenOpens.map(\.pin), ["1234"])
        XCTAssertEqual(try Data(contentsOf: destination), client.signedBytes)
    }

    func testAnEmptyPinLeavesAuthenticationToTheToken() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = tokenModel(client, module: URL(fileURLWithPath: "/m.so"), pin: "", destination: nil)

        model.signWithToken()

        waitUntil({ !client.tokenOpens.isEmpty }, "the token was never opened")
        XCTAssertEqual(client.tokenOpens.map(\.pin), [nil])
    }

    func testCancellingTheModuleOrThePinOpensNothing() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let noModule = tokenModel(client, module: nil, pin: "1", destination: nil)
        noModule.signWithToken()
        XCTAssertEqual(noModule.store.editStatus, "Certificate selection cancelled.")

        let noPin = tokenModel(client, module: URL(fileURLWithPath: "/m.so"), pin: nil, destination: nil)
        noPin.signWithToken()
        XCTAssertEqual(noPin.store.editStatus, "Certificate selection cancelled.")
        XCTAssertTrue(client.tokenOpens.isEmpty)
    }

    func testATokenThatWillNotOpenSaysWhy() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.tokenError = SigningSourceFailure.failed("The PKCS#11 module could not be loaded.")
        let model = tokenModel(client, module: URL(fileURLWithPath: "/m.so"), pin: "1", destination: nil)

        model.signWithToken()

        waitUntil({ model.store.editStatus == "The PKCS#11 module could not be loaded." }, "the failure was never reported")
        XCTAssertTrue(client.signedAs.isEmpty)
    }
}
