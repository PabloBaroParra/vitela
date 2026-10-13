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
