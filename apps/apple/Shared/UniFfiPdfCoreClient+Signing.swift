// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Digital signing through the generated UniFFI API — the calls Windows'
/// facade makes (`PdfDocumentFacade.Signing.cs`).
extension UniFfiPdfCoreClient {
    func signingRefusal(document: any PdfDocument) -> String? {
        guard let handle = try? handle(of: document) else { return "The document is no longer open." }
        return Vitela.signingRefusal(handle: handle).map(EditFailure.sentence)
    }

    func openSigningCertificate(bytes: Data, password: String) throws -> any SigningSource {
        do {
            return UniFfiSigningSource(certificate: try Vitela.openSigningCertificate(bytes: bytes, password: password))
        } catch FfiError.WrongPassword {
            throw SigningSourceFailure.wrongPassword
        } catch let error as FfiError {
            throw SigningSourceFailure.failed(EditFailure(error).localizedDescription)
        }
    }

    func openTokenSigningSource(modulePath: String, pin: String?) throws -> any SigningSource {
        do {
            return UniFfiSigningSource(certificate: try Vitela.openTokenSigningSource(modulePath: modulePath, pin: pin))
        } catch let error as FfiError {
            throw SigningSourceFailure.failed(EditFailure(error).localizedDescription)
        }
    }

    func openNativeSigningSource(_ store: any NativeSigningStore) throws -> any SigningSource {
        do {
            return UniFfiSigningSource(certificate: try Vitela.openPlatformSigningSource(source: UniFfiPlatformSigningSource(store: store)))
        } catch let error as FfiError {
            throw SigningSourceFailure.failed(EditFailure(error).localizedDescription)
        }
    }

    func sign(document: any PdfDocument, source: any SigningSource, identityId: String) throws -> Data {
        let handle = try handle(of: document)
        guard let source = source as? UniFfiSigningSource else {
            throw EditFailure.failed("signing source is not backed by UniFFI")
        }
        return try Self.mapEditErrors { try signToBytes(handle: handle, certificate: source.certificate, identityId: identityId) }
    }

    func reopenSigned(document: any PdfDocument, bytes: Data) throws -> any PdfDocument {
        let handle = try handle(of: document)
        return try Self.document { try reopenSignedDocument(handle: handle, bytes: bytes) }
    }
}

/// Wraps the generated `SigningCertificate`, which keeps its adapter alive
/// for as long as the shell holds it.
struct UniFfiSigningSource: SigningSource {
    let certificate: SigningCertificate

    var identities: [SigningIdentity] {
        certificate.identities().map { SigningIdentity(id: $0.id, displayName: $0.displayName) }
    }
}

/// Hands a `NativeSigningStore` to the core as its `PlatformSigningSource`.
/// The core keeps it for as long as the `SigningCertificate` lives, and calls
/// it from its own threads. Whatever the store throws reaches the reader as
/// the core's refusal, so it must be a sentence.
final class UniFfiPlatformSigningSource: PlatformSigningSource, @unchecked Sendable {
    private let store: any NativeSigningStore

    init(store: any NativeSigningStore) {
        self.store = store
    }

    func identities() throws -> [FfiPlatformSigningIdentity] {
        do {
            return try store.identities().map {
                FfiPlatformSigningIdentity(
                    id: $0.id, displayName: $0.displayName, certificateChainDer: $0.certificateChainDer,
                    algorithm: FfiSigningAlgorithm($0.algorithm)
                )
            }
        } catch {
            throw FfiError.UnsupportedOperation(detail: error.localizedDescription)
        }
    }

    func signDigest(identityId: String, digest: Data, algorithm: FfiSigningAlgorithm) throws -> Data {
        do {
            return try store.signDigest(identityId: identityId, digest: digest, algorithm: NativeSigningAlgorithm(algorithm))
        } catch {
            throw FfiError.UnsupportedOperation(detail: error.localizedDescription)
        }
    }
}

private extension FfiSigningAlgorithm {
    init(_ algorithm: NativeSigningAlgorithm) {
        self = algorithm == .rsaSha256 ? .rsaSha256 : .ecdsaSha256
    }
}

private extension NativeSigningAlgorithm {
    init(_ algorithm: FfiSigningAlgorithm) {
        self = algorithm == .rsaSha256 ? .rsaSha256 : .ecdsaSha256
    }
}
