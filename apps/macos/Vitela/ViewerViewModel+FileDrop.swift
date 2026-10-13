import Foundation

/// Files handed to the app from outside: dropped on the window, or sent by
/// Finder (double-click, Open With, a drop on the Dock icon). Mirrors the
/// window-level drop in Windows' `MainWindow.FileDrop.cs`: only a PDF has a
/// meaning here, and it replaces the open document after the usual
/// unsaved-changes question.
extension ViewerViewModel {
    static let unsupportedDropMessage = "Vitela opens PDF files."

    /// Opens the first file of `urls` if its content is a PDF. Reading runs
    /// on the operation queue; the decision comes back to main.
    func openExternalFiles(_ urls: [URL]) {
        guard let url = DroppedFileKind.firstFile(urls) else {
            notice = Self.unsupportedDropMessage
            return
        }
        operationQueue.addOperation { [weak self] in
            let bytes = try? Data(contentsOf: url)
            DispatchQueue.main.async { self?.openExternal(bytes: bytes, url: url) }
        }
    }

    private func openExternal(bytes: Data?, url: URL) {
        guard let bytes else {
            notice = "The dropped file could not be read."
            return
        }
        guard DroppedFileKind.classify(bytes) == .document else {
            notice = Self.unsupportedDropMessage
            return
        }
        proceedPastUnsavedChanges { [weak self] in
            guard let self else { return }
            self.notice = ""
            self.documentName = url.lastPathComponent
            self.pendingRecentPath = url.path
            self.store.open(bytes: bytes)
        }
    }
}
