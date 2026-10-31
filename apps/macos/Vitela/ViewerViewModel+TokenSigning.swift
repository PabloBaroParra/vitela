import AppKit
import Foundation

/// The modal side of signing with a card or token.
enum TokenSigningPrompts {
    /// Where OpenSC, the usual smart-card middleware on a Mac, installs its
    /// module; the panel opens there when it exists.
    static let openScModules = URL(fileURLWithPath: "/Library/OpenSC/lib", isDirectory: true)

    static func chooseModule() -> URL? {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.prompt = "Choose"
        panel.message = "Choose the card or token's PKCS#11 module (a .dylib or .so file from its middleware)."
        if FileManager.default.fileExists(atPath: openScModules.path) { panel.directoryURL = openScModules }
        return panel.runModal() == .OK ? panel.url : nil
    }

    /// Windows' "Card or token authentication" dialog, Cancel the default.
    static func askPin() -> String? {
        let alert = NSAlert()
        alert.messageText = "Card or token authentication"
        alert.informativeText = "Enter the token PIN, or leave it empty to use the token's own authentication prompt."
        let field = NSSecureTextField(frame: NSRect(x: 0, y: 0, width: 240, height: 24))
        alert.accessoryView = field
        alert.addButton(withTitle: "Cancel")
        alert.addButton(withTitle: "Continue")
        alert.window.initialFirstResponder = field
        return alert.runModal() == .alertSecondButtonReturn ? field.stringValue : nil
    }
}

/// File ▸ Sign with Card or Token…: a PKCS#11 module, its PIN, then the same
/// identity, destination and write as a certificate file — Windows' "card
/// certificate" source (`MainWindow.SigningSources.cs`,
/// `AskTokenSigningCertificateAsync`). The core loads the module and keeps
/// the keys on the token.
///
/// The Mac App Store build does not offer it (`offersTokenSigning`): a
/// PKCS#11 module is native code from the token vendor, loaded from outside
/// the app bundle, which the App Sandbox and App Review do not allow. The
/// Developer ID build keeps it, and both builds still sign with a
/// certificate file or a Keychain identity.
extension ViewerViewModel {
    func signWithToken() {
        guard offersTokenSigning, store.document != nil, !isSaving else { return }
        if let refusal = store.signingRefusal {
            store.editStatus = refusal
            return
        }
        guard let module = signingPrompts.chooseTokenModule() else {
            store.editStatus = "Certificate selection cancelled."
            return
        }
        guard let typed = signingPrompts.askTokenPin() else {
            store.editStatus = "Certificate selection cancelled."
            return
        }
        // An empty PIN lets the token authenticate on its own (a PIN pad,
        // the middleware's dialog), as on Windows.
        let pin = typed.isEmpty ? nil : typed
        store.editStatus = "Opening card or token…"
        let client = store.client
        operationQueue.addOperation { [weak self] in
            let opened = Result { try client.openTokenSigningSource(modulePath: module.path, pin: pin) }
            DispatchQueue.main.async {
                guard let self else { return }
                switch opened {
                case let .success(source):
                    self.chooseIdentity(in: source)
                case let .failure(error):
                    self.store.editStatus = error.localizedDescription
                }
            }
        }
    }
}
