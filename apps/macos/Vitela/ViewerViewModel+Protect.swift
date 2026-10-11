import Foundation

/// File ▸ Protect with Password…: the macOS chain around
/// `ViewerStore+Protect.swift`. Mirrors Windows' `MainWindow.Protect.cs`:
/// the permission gate, the two passwords, the protection's own signature
/// question, a destination, then the encrypted bytes off the main thread,
/// written atomically and reopened.
extension ViewerViewModel {
    func requestProtect() {
        guard store.document != nil, !isSaving else { return }
        if let refusal = store.protectionRefusal() {
            store.editStatus = refusal
            return
        }
        isProtectPresented = true
    }

    func cancelProtect() {
        isProtectPresented = false
        store.editStatus = "Protection cancelled."
    }

    /// Returns what is wrong with the passwords for the sheet to show, or
    /// `nil` once the sheet can close and the rest of the chain run.
    func protect(openPassword: String, permissionsPassword: String) -> String? {
        if let problem = ViewerStore.protectionPasswordProblem(open: openPassword, permissions: permissionsPassword) {
            return problem
        }
        guard let request = store.beginProtect(openPassword: openPassword, permissionsPassword: permissionsPassword) else {
            return store.protectionRefusal()
        }
        isProtectPresented = false
        isSaving = true
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let invalidates = self.store.protectionWillInvalidateSignatures(for: request)
            DispatchQueue.main.async {
                switch invalidates {
                case let .failure(failure):
                    self.finishProtect(failure.localizedDescription)
                case .success(true) where !self.prompts.confirmSignatureLoss():
                    self.finishProtect("Protection cancelled.")
                case let .success(invalidates):
                    self.writeProtected(request, acknowledgingSignatureLoss: invalidates)
                }
            }
        }
        return nil
    }

    private func writeProtected(_ request: ViewerStore.ProtectRequest, acknowledgingSignatureLoss: Bool) {
        guard let destination = prompts.chooseDestination("document.pdf") else {
            return finishProtect("Protection cancelled.")
        }
        store.editStatus = "Protecting PDF…"
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let outcome = self.store.protectResult(for: request, acknowledgingSignatureLoss: acknowledgingSignatureLoss)
                .flatMap { bytes -> Result<Data, EditFailure> in
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
                    if !self.store.reopenProtected(bytes, for: request) {
                        self.documentName = name
                    }
                }
            }
        }
    }

    private func finishProtect(_ status: String) {
        isSaving = false
        store.editStatus = status
    }
}
