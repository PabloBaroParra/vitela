import AppKit
import Foundation

/// The modal questions saving asks, as closures so tests can answer them
/// without a window. `.modal` is the real AppKit implementation.
struct SavePrompts {
    enum UnsavedChoice {
        case save, discard, cancel
    }

    /// Where to write, given a suggested file name; `nil` when cancelled.
    var chooseDestination: (_ suggestedName: String) -> URL?
    /// Whether the user accepts that saving breaks the file's signature.
    var confirmSignatureLoss: () -> Bool
    /// What to do with unsaved changes before they would be lost.
    var resolveUnsavedChanges: () -> UnsavedChoice

    static let modal = SavePrompts(
        chooseDestination: { suggestedName in
            let panel = NSSavePanel()
            panel.allowedContentTypes = [.pdf]
            panel.nameFieldStringValue = suggestedName
            panel.prompt = "Save"
            return panel.runModal() == .OK ? panel.url : nil
        },
        confirmSignatureLoss: {
            // Same wording as the Windows shell's `AskSignatureLossAsync`, and
            // Cancel is the default there too.
            let alert = NSAlert()
            alert.alertStyle = .warning
            alert.messageText = "Saving will break this document's signature"
            alert.informativeText = """
                This document is signed. Saving rewrites the file, so the signature will no longer match what it covers.

                It is not removed: the saved file still carries the signature, and PDF readers will report it as invalid rather than missing.

                To keep a copy that still verifies, cancel and save to a different file.
                """
            alert.addButton(withTitle: "Cancel")
            alert.addButton(withTitle: "Save Anyway")
            return alert.runModal() == .alertSecondButtonReturn
        },
        resolveUnsavedChanges: {
            let alert = NSAlert()
            alert.messageText = "Do you want to save the changes made to this document?"
            alert.informativeText = "Your changes will be lost if you don't save them."
            alert.addButton(withTitle: "Save…")
            alert.addButton(withTitle: "Cancel")
            alert.addButton(withTitle: "Don't Save")
            switch alert.runModal() {
            case .alertFirstButtonReturn: return .save
            case .alertThirdButtonReturn: return .discard
            default: return .cancel
            }
        }
    )
}

/// Save As, and the unsaved-changes guard in front of anything that would
/// replace the open document. Mirrors `MainWindow.Save.cs`: pick the
/// destination, ask about signatures, compute the bytes off the main thread,
/// then replace the destination atomically so a failed write never leaves a
/// half-written file behind.
extension ViewerViewModel {
    /// Saves to a file the user picks. `completion` reports whether the bytes
    /// reached disk, so a caller waiting to discard work only does so then.
    func saveAs(completion: @escaping (Bool) -> Void = { _ in }) {
        guard !isSaving, store.document != nil else { return completion(false) }
        guard let destination = prompts.chooseDestination(suggestedSaveName) else {
            store.reportSaveFailure("Save cancelled.")
            return completion(false)
        }
        guard let probe = store.beginSave(acknowledgingSignatureLoss: false) else { return completion(false) }
        isSaving = true
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let invalidates = self.store.saveWillInvalidateSignatures(for: probe)
            DispatchQueue.main.async {
                switch invalidates {
                case let .failure(failure):
                    self.finishSave(failure.localizedDescription, completion: completion)
                case .success(true) where !self.prompts.confirmSignatureLoss():
                    self.finishSave("Save cancelled.", completion: completion)
                case let .success(invalidates):
                    self.write(to: destination, acknowledgingSignatureLoss: invalidates, completion: completion)
                }
            }
        }
    }

    /// Answers `true` once the open document's unsaved changes are saved or
    /// knowingly discarded, `false` if the user cancels or the save fails.
    /// The answer is synchronous except after choosing Save.
    func resolveUnsavedChanges(then decide: @escaping (_ proceed: Bool) -> Void) {
        guard !isSaving else { return decide(false) }
        guard store.hasUnsavedChanges else { return decide(true) }
        switch prompts.resolveUnsavedChanges() {
        case .discard: decide(true)
        case .cancel: decide(false)
        case .save: saveAs(completion: decide)
        }
    }

    /// `resolveUnsavedChanges` for callers that only act on a yes.
    func proceedPastUnsavedChanges(_ proceed: @escaping () -> Void) {
        resolveUnsavedChanges { if $0 { proceed() } }
    }

    func undo() { store.undo() }

    func redo() { store.redo() }

    private var suggestedSaveName: String {
        guard let name = documentName, !name.isEmpty else { return "document.pdf" }
        return name.lowercased().hasSuffix(".pdf") ? name : name + ".pdf"
    }

    private func write(to destination: URL, acknowledgingSignatureLoss: Bool, completion: @escaping (Bool) -> Void) {
        guard let request = store.beginSave(acknowledgingSignatureLoss: acknowledgingSignatureLoss) else {
            return finishSave("The document is no longer open.", completion: completion)
        }
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let outcome = self.store.saveResult(for: request).flatMap { bytes -> Result<Void, EditFailure> in
                // `.atomic` writes a sibling temporary file and renames it over
                // the destination, the same replace-on-success the Windows
                // shell does by hand.
                Result { try bytes.write(to: destination, options: .atomic) }
                    .mapError { .failed("Could not write the file: \($0.localizedDescription)") }
            }
            DispatchQueue.main.async {
                switch outcome {
                case .success:
                    self.store.applySaved(request)
                    self.isSaving = false
                    completion(true)
                case let .failure(failure):
                    self.finishSave(failure.localizedDescription, completion: completion)
                }
            }
        }
    }

    private func finishSave(_ message: String, completion: (Bool) -> Void) {
        store.reportSaveFailure(message)
        isSaving = false
        completion(false)
    }
}
