import AppKit
import Foundation

/// What sharing asks and shows, as closures so tests can answer without a
/// window. `.modal` is the real AppKit implementation.
struct SharePrompts {
    /// Whether the user accepts that the shared copy breaks the signature.
    var confirmSignatureLoss: () -> Bool
    /// Shows the share picker for `file`; `false` when it could not be shown.
    var present: (_ file: URL) -> Bool
    /// Where shared copies are written, one fresh folder per share.
    var root: URL = FileManager.default.temporaryDirectory
        .appendingPathComponent("Vitela", isDirectory: true)
        .appendingPathComponent("Share", isDirectory: true)

    static let modal = SharePrompts(
        confirmSignatureLoss: {
            // Windows' `AskSignatureLossAsync(sharing: true)` wording; Cancel
            // is the default there too.
            let alert = NSAlert()
            alert.alertStyle = .warning
            alert.messageText = "Sharing will break this document's signature"
            alert.informativeText = """
                This document is signed. Sharing sends a rewritten copy, so its signature will no longer match what it covers.

                It is not removed: the copy still carries the signature, and PDF readers will report it as invalid rather than missing.

                To send a copy that still verifies, cancel and share the original file instead.
                """
            alert.addButton(withTitle: "Cancel")
            alert.addButton(withTitle: "Share Anyway")
            return alert.runModal() == .alertSecondButtonReturn
        },
        present: { file in
            // The picker hangs off a view; the window's top edge, beside the
            // toolbar's Share button, is where the reader is looking.
            guard let view = NSApp.keyWindow?.contentView ?? NSApp.mainWindow?.contentView else { return false }
            let anchor = NSRect(x: view.bounds.maxX - 120, y: view.isFlipped ? 0 : view.bounds.maxY - 1, width: 1, height: 1)
            NSSharingServicePicker(items: [file]).show(relativeTo: anchor, of: view, preferredEdge: .minY)
            return true
        }
    )
}

/// File ▸ Share…: the document as a save would write it, pending edits
/// included, handed to the macOS share picker (Mail, Messages, AirDrop and
/// any installed extension). Mirrors Windows' `MainWindow.Share.cs`: the
/// signature question first, the bytes computed off the main thread, written
/// to a fresh private folder the target reads from, and copies older than
/// an hour swept on the next share — the picker returns while a target may
/// still be reading the file.
extension ViewerViewModel {
    /// How long a shared copy outlives its share — Windows' `ShareCopyLifetime`.
    static let shareCopyLifetime: TimeInterval = 60 * 60

    var canShare: Bool { store.pageCount > 0 && !isSaving }

    func share() {
        guard !isSaving else { return }
        guard store.pageCount > 0, let probe = store.beginSave(acknowledgingSignatureLoss: false) else {
            store.editStatus = "Open a PDF before sharing."
            return
        }
        isSaving = true
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let invalidates = self.store.saveWillInvalidateSignatures(for: probe)
            DispatchQueue.main.async {
                switch invalidates {
                case let .failure(failure):
                    self.finishShare("Could not share: \(failure.localizedDescription)")
                case .success(true) where !self.sharePrompts.confirmSignatureLoss():
                    self.finishShare("Share cancelled.")
                case let .success(invalidates):
                    self.writeSharedCopy(acknowledgingSignatureLoss: invalidates)
                }
            }
        }
    }

    private func writeSharedCopy(acknowledgingSignatureLoss: Bool) {
        guard let request = store.beginSave(acknowledgingSignatureLoss: acknowledgingSignatureLoss) else {
            return finishShare("The document is no longer open.")
        }
        let fileName = Self.shareFileName(for: documentName ?? "")
        let root = sharePrompts.root
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let written: Result<URL, Error> = self.store.saveResult(for: request)
                .mapError { $0 as Error }
                .flatMap { bytes in Result { try Self.writeShareCopy(bytes, named: fileName, under: root) } }
            DispatchQueue.main.async {
                switch written {
                case let .failure(error):
                    self.finishShare("Could not share: \(error.localizedDescription)")
                case let .success(file):
                    self.finishShare(self.sharePrompts.present(file) ? "" : "Sharing is unavailable right now.")
                }
            }
        }
    }

    private func finishShare(_ status: String) {
        isSaving = false
        store.editStatus = status
    }

    /// `"report"` → `"report.pdf"`; never a path, never empty — Windows'
    /// `ShareFileName.For` with the characters a macOS file name cannot hold.
    static func shareFileName(for displayName: String) -> String {
        let cleaned = String(displayName.trimmingCharacters(in: .whitespacesAndNewlines).map { "/:".contains($0) ? "_" : $0 })
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let name = cleaned.isEmpty || cleaned == "." || cleaned == ".." ? "document" : cleaned
        return name.lowercased().hasSuffix(".pdf") ? name : name + ".pdf"
    }

    /// One fresh folder per share, so two shares never race over one name.
    static func writeShareCopy(_ bytes: Data, named fileName: String, under root: URL, now: Date = Date()) throws -> URL {
        sweepShareCopies(under: root, now: now)
        let folder = root.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let file = folder.appendingPathComponent(fileName)
        try bytes.write(to: file, options: .atomic)
        return file
    }

    /// Removes share folders older than `shareCopyLifetime`. One a target
    /// still holds stays for the next sweep.
    static func sweepShareCopies(under root: URL, now: Date = Date()) {
        let folders = (try? FileManager.default.contentsOfDirectory(
            at: root, includingPropertiesForKeys: [.creationDateKey], options: [.skipsHiddenFiles]
        )) ?? []
        for folder in folders {
            guard let created = try? folder.resourceValues(forKeys: [.creationDateKey]).creationDate,
                  now.timeIntervalSince(created) > shareCopyLifetime
            else { continue }
            try? FileManager.default.removeItem(at: folder)
        }
    }
}
