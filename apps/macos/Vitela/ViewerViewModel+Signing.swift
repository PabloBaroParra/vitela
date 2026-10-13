import AppKit
import Foundation
import UniformTypeIdentifiers

/// The questions signing asks, as closures so tests can answer them without
/// a window. `.modal` is the real AppKit implementation.
struct SigningPrompts {
    /// A `.pfx`/`.p12` file; `nil` when cancelled.
    var chooseCertificate: () -> URL?
    /// The certificate's password; `nil` when cancelled. `retry` says the
    /// last one did not unlock it.
    var askPassword: (_ retry: Bool) -> String?
    /// Which identity to sign as; `nil` when cancelled.
    var chooseIdentity: (_ identities: [SigningIdentity]) -> SigningIdentity?
    /// A card or token's PKCS#11 module; `nil` when cancelled.
    var chooseTokenModule: () -> URL? = TokenSigningPrompts.chooseModule
    /// The token's PIN; `""` leaves it to the token's own prompt, `nil` cancels.
    var askTokenPin: () -> String? = TokenSigningPrompts.askPin

    static let modal = SigningPrompts(
        chooseCertificate: {
            let panel = NSOpenPanel()
            panel.allowedContentTypes = ["pfx", "p12"].compactMap { UTType(filenameExtension: $0) }
            panel.prompt = "Choose"
            panel.message = "Choose a PKCS#12 signing certificate (.pfx or .p12)."
            return panel.runModal() == .OK ? panel.url : nil
        },
        askPassword: { retry in
            let alert = NSAlert()
            alert.messageText = "Certificate password required"
            alert.informativeText = retry
                ? "The password is incorrect, or the file is not a valid PKCS#12 certificate."
                : "Enter the password that protects this certificate."
            let field = NSSecureTextField(frame: NSRect(x: 0, y: 0, width: 240, height: 24))
            alert.accessoryView = field
            alert.addButton(withTitle: "Unlock")
            alert.addButton(withTitle: "Cancel")
            alert.window.initialFirstResponder = field
            return alert.runModal() == .alertFirstButtonReturn ? field.stringValue : nil
        },
        chooseIdentity: { identities in
            let alert = NSAlert()
            alert.messageText = "Choose a signing identity"
            alert.informativeText = "Sign this document with:"
            let menu = NSPopUpButton(frame: NSRect(x: 0, y: 0, width: 280, height: 26))
            menu.addItems(withTitles: identities.map(\.displayName))
            alert.accessoryView = menu
            alert.addButton(withTitle: "Sign")
            alert.addButton(withTitle: "Cancel")
            guard alert.runModal() == .alertFirstButtonReturn, identities.indices.contains(menu.indexOfSelectedItem) else { return nil }
            return identities[menu.indexOfSelectedItem]
        }
    )
}

/// File ▸ Sign with Certificate…: certificate, password, identity,
/// destination, then the signed bytes off the main thread, written
/// atomically and reopened — Windows' `ChooseSigningCertificate_Click`.
extension ViewerViewModel {
    func signWithCertificateFile() {
        guard store.document != nil, !isSaving else { return }
        if let refusal = store.signingRefusal {
            store.editStatus = refusal
            return
        }
        guard let url = signingPrompts.chooseCertificate() else {
            store.editStatus = "Certificate selection cancelled."
            return
        }
        guard let bytes = try? Data(contentsOf: url) else {
            store.editStatus = "The certificate file could not be read."
            return
        }
        unlock(bytes, retry: false)
    }

    private func unlock(_ bytes: Data, retry: Bool) {
        guard let password = signingPrompts.askPassword(retry) else {
            store.editStatus = "Certificate selection cancelled."
            return
        }
        store.editStatus = "Reading certificate file…"
        let client = store.client
        operationQueue.addOperation { [weak self] in
            let opened = Result { try client.openSigningCertificate(bytes: bytes, password: password) }
            DispatchQueue.main.async {
                guard let self else { return }
                switch opened {
                case let .success(source):
                    self.chooseIdentity(in: source)
                case .failure(SigningSourceFailure.wrongPassword):
                    self.unlock(bytes, retry: true)
                case let .failure(error):
                    self.store.editStatus = error.localizedDescription
                }
            }
        }
    }

    /// The rest of every signing path once a source is open: identity,
    /// destination, sign, write, reopen.
    func chooseIdentity(in source: any SigningSource) {
        let identities = source.identities
        guard !identities.isEmpty else {
            store.editStatus = "This certificate source has no usable signing identities."
            return
        }
        guard let identity = signingPrompts.chooseIdentity(identities) else {
            store.editStatus = "Signing cancelled."
            return
        }
        let stem = ((documentName ?? "document") as NSString).deletingPathExtension
        guard let destination = prompts.chooseDestination("\(stem)-signed.pdf") else {
            store.editStatus = "Signing cancelled."
            return
        }
        guard let request = store.beginSign(source: source, identityId: identity.id) else { return }
        isSaving = true
        store.editStatus = "Signing document…"
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let outcome = self.store.signResult(for: request).flatMap { bytes -> Result<Data, EditFailure> in
                Result { try bytes.write(to: destination, options: .atomic); return bytes }
                    .mapError { .failed("Could not write the file: \($0.localizedDescription)") }
            }
            DispatchQueue.main.async {
                self.isSaving = false
                switch outcome {
                case let .failure(failure):
                    self.store.editStatus = failure.localizedDescription
                case let .success(bytes):
                    let name = self.documentName
                    self.documentName = destination.lastPathComponent
                    if !self.store.reopenSigned(bytes, for: request) { self.documentName = name }
                }
            }
        }
    }
}
