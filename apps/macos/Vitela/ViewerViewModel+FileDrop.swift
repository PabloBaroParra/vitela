import Foundation

/// Files handed to the app from outside: dropped on the window, or sent by
/// Finder (double-click, Open With, a drop on the Dock icon). Mirrors the
/// window-level drop in Windows' `MainWindow.FileDrop.cs`: a PDF replaces
/// the open document after the usual unsaved-changes question; an image
/// means something only dropped on a page (`ViewerViewModel+PasteImage.swift`).
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
        switch DroppedFileKind.classify(bytes) {
        case .document:
            break
        case .image:
            // Windows' wording: an image has a meaning only on a page.
            notice = store.document == nil
                ? "Open a PDF first, then drop an image onto a page to stamp it."
                : "Drop the image onto a page to place it as a stamp."
            return
        case .unsupported:
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
