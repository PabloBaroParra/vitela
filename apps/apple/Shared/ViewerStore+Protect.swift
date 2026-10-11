// Protect with a password: write an encrypted copy of the document and
// reopen it, so what is on screen is the protected file. Mirrors Windows'
// `MainWindow.Protect.cs` (its checks and wording) and the facade's
// `ProtectToDestinationAsync`/`ReopenProtectedAsync`.
import Foundation

extension ViewerStore {
    struct ProtectRequest {
        let generation: UInt
        let document: any PdfDocument
        let openPassword: String
        let permissionsPassword: String
    }

    /// Why protection cannot start for this document, or `nil`.
    func protectionRefusal() -> String? {
        guard document != nil else { return "The document is no longer open." }
        return editingAllowed ? nil : "This document does not permit changes to its protection."
    }

    /// What is wrong with a pair of typed passwords, or `nil` — Windows'
    /// checks, in its order.
    static func protectionPasswordProblem(open: String, permissions: String) -> String? {
        if open.isEmpty { return "Enter the password the document will ask for when it is opened." }
        if permissions.isEmpty { return "Enter the permissions password." }
        if open == permissions {
            return "The two passwords must be different. If they were the same, everyone who can open the document could also change what it permits."
        }
        return nil
    }

    func beginProtect(openPassword: String, permissionsPassword: String) -> ProtectRequest? {
        guard let document, protectionRefusal() == nil,
              Self.protectionPasswordProblem(open: openPassword, permissions: permissionsPassword) == nil
        else { return nil }
        return ProtectRequest(
            generation: generation, document: document, openPassword: openPassword, permissionsPassword: permissionsPassword
        )
    }

    /// Thread-safe: only the immutable client and the request's own document.
    func protectionWillInvalidateSignatures(for request: ProtectRequest) -> Result<Bool, EditFailure> {
        Result { try client.protectionWillInvalidateSignatures(document: request.document) }.mapError(Self.editFailure)
    }

    /// Thread-safe: only the immutable client and the request's own document.
    func protectResult(for request: ProtectRequest, acknowledgingSignatureLoss: Bool) -> Result<Data, EditFailure> {
        Result {
            try client.protect(
                document: request.document,
                openPassword: request.openPassword,
                permissionsPassword: request.permissionsPassword,
                acknowledgingSignatureLoss: acknowledgingSignatureLoss
            )
        }
        .mapError(Self.editFailure)
    }

    /// Shows the protected file just written, opened with both passwords so
    /// it stays editable here. A request for a document no longer on screen
    /// changes nothing.
    @discardableResult
    func reopenProtected(_ bytes: Data, for request: ProtectRequest) -> Bool {
        guard request.generation == generation else { return false }
        open(bytes: bytes, userPassword: request.openPassword, ownerPassword: request.permissionsPassword)
        guard state == .loaded else { return false }
        editStatus = "Protected PDF saved and reopened."
        return true
    }
}
