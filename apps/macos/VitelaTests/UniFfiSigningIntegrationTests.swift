import XCTest
@testable import Vitela

/// Signing against the real core with a throwaway self-signed certificate,
/// made by the system's LibreSSL for this run — the Rust tests build theirs
/// with `gen_fixtures`, which the Swift side cannot reach.
final class UniFfiSigningIntegrationTests: XCTestCase {
    private func makeP12(password: String) throws -> Data {
        let openssl = URL(fileURLWithPath: "/usr/bin/openssl")
        guard FileManager.default.isExecutableFile(atPath: openssl.path) else { throw XCTSkip("no /usr/bin/openssl to make a test certificate") }
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("vitela-p12-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        func run(_ arguments: [String]) throws {
            let process = Process()
            process.executableURL = openssl
            process.arguments = arguments
            process.currentDirectoryURL = folder
            process.standardError = FileHandle.nullDevice
            process.standardOutput = FileHandle.nullDevice
            try process.run()
            process.waitUntilExit()
            guard process.terminationStatus == 0 else { throw XCTSkip("openssl \(arguments.first ?? "") failed") }
        }
        // A named P-256 curve: without `ec_param_enc:named_curve`, LibreSSL
        // writes explicit curve parameters, which RFC 5480 rules out and the
        // core does not take.
        try run(["req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1",
                 "-pkeyopt", "ec_param_enc:named_curve", "-nodes",
                 "-keyout", "key.pem", "-out", "cert.pem", "-days", "2", "-subj", "/CN=Vitela Test Signer"])
        // 3DES rather than LibreSSL's default RC2-40 for the certificate
        // bag, which the core's PKCS#12 reader does not decrypt.
        try run(["pkcs12", "-export", "-inkey", "key.pem", "-in", "cert.pem", "-out", "test.p12", "-passout", "pass:\(password)",
                 "-keypbe", "PBE-SHA1-3DES", "-certpbe", "PBE-SHA1-3DES"])
        return try Data(contentsOf: folder.appendingPathComponent("test.p12"))
    }

    func testAWrongPasswordDoesNotUnlockAndTheRightOneSigns() throws {
        let p12 = try makeP12(password: "vitela-test")
        try offMain {
            let client = UniFfiPdfCoreClient()
            XCTAssertThrowsError(try client.openSigningCertificate(bytes: p12, password: "nope")) {
                XCTAssertEqual($0 as? SigningSourceFailure, .wrongPassword)
            }
            let source = try client.openSigningCertificate(bytes: p12, password: "vitela-test")
            let identity = try XCTUnwrap(source.identities.first)
            XCTAssertTrue(identity.displayName.contains("Vitela Test Signer"))

            let store = ViewerStore(client: client)
            store.open(bytes: try Self.sampleBytes())
            XCTAssertNil(store.signingRefusal)
            XCTAssertFalse(store.isSigned)
            let request = try XCTUnwrap(store.beginSign(source: source, identityId: identity.id))
            let signed = try store.signResult(for: request).get()
            XCTAssertNotNil(signed.range(of: Data("/ByteRange".utf8)))

            XCTAssertTrue(store.reopenSigned(signed, for: request))
            XCTAssertTrue(store.isSigned, "the reopened file is the signed one")
        }
    }
}
