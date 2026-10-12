// Digital signing: sign the document with an identity from an unlocked
// source, write the signed copy, and reopen it so what is on screen is the
// signed file. Mirrors Windows' `MainWindow.Signing.cs` and the facade's
// `SignToDestinationAsync`; the signature itself — the field, the digest,
// the CMS — is the core's.
import Foundation

extension ViewerStore {
    struct SignRequest {
        let generation: UInt
        let document: any PdfDocument
        let source: any SigningSource
        let identityId: String
    }

    /// Starts signing, or reports why it cannot and returns `nil`.
    func beginSign(source: any SigningSource, identityId: String) -> SignRequest? {
        guard let document else { return nil }
        if let refusal = signingRefusal {
            editStatus = refusal
            return nil
        }
        return SignRequest(generation: generation, document: document, source: source, identityId: identityId)
    }

    /// Thread-safe: only the immutable client and the request's own values.
    func signResult(for request: SignRequest) -> Result<Data, EditFailure> {
        Result { try client.sign(document: request.document, source: request.source, identityId: request.identityId) }
            .mapError(Self.editFailure)
    }

    /// Shows the signed file just written in place of the one it was signed
    /// from. A request for a document no longer on screen changes nothing.
    @discardableResult
    func reopenSigned(_ bytes: Data, for request: SignRequest) -> Bool {
        guard request.generation == generation else { return false }
        openSigned(bytes, from: request.document)
        guard state == .loaded else { return false }
        editStatus = "Signed PDF saved and reopened."
        return true
    }
}
