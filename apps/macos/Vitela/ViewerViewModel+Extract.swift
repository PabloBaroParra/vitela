import Foundation

/// File ▸ Extract Pages…: the macOS chain around `ViewerStore+Extract.swift`.
/// Mirrors Windows' `MainWindow.ExtractPages.cs` — check the gates before
/// asking anything, keep the range sheet open on a refusal, only then pick a
/// destination, and write through a temporary file so a failure never leaves
/// a half-written PDF at the chosen name.
extension ViewerViewModel {
    /// Windows and Linux suggest the same neutral name, which also keeps the
    /// panel from proposing to overwrite the document being read.
    static let extractSuggestedName = "document.pdf"

    /// Whether Extract can be asked for now — the File menu's item and the
    /// Organize header's button share it, so the two cannot drift.
    var canRequestExtract: Bool { store.document != nil && !isSaving }

    func requestExtractPages() {
        guard store.document != nil, !isSaving else { return }
        // The permission gates do not depend on the range; checking them
        // with a range every document has spares the reader a dialog that
        // could never succeed.
        if case let .failure(failure) = store.planExtract(range: "1") {
            store.editStatus = failure.localizedDescription
            return
        }
        isExtractPresented = true
    }

    func cancelExtractPages() {
        isExtractPresented = false
        store.editStatus = "Extract cancelled."
    }

    /// Plans `range`. Returns the refusal for the sheet to show, or `nil`
    /// once the sheet can close and the save panel take over.
    func extractPages(range: String) -> String? {
        switch store.planExtract(range: range) {
        case let .failure(failure):
            return failure.localizedDescription
        case let .success(plan):
            isExtractPresented = false
            // After the sheet has gone: a save panel run modally from
            // inside a sheet's button action would stack on top of it.
            DispatchQueue.main.async { [weak self] in self?.writeExtract(plan) }
            return nil
        }
    }

    private func writeExtract(_ plan: ViewerStore.ExtractPlan) {
        guard !isSaving else { return }
        guard let destination = prompts.chooseDestination(Self.extractSuggestedName) else {
            store.editStatus = "Extract cancelled. No file was written."
            return
        }
        guard let request = store.beginExtract(plan) else { return }
        isSaving = true
        let count = plan.pages.count
        store.editStatus = "Extracting \(count) \(count == 1 ? "page" : "pages")…"
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let outcome = self.store.extractResult(for: request).flatMap { bytes -> Result<Void, EditFailure> in
                Result { try bytes.write(to: destination, options: .atomic) }
                    .mapError { .failed("Could not write the file: \($0.localizedDescription)") }
            }
            DispatchQueue.main.async {
                self.isSaving = false
                switch outcome {
                case .success:
                    self.store.editStatus = ViewerStore.extractSummary(
                        count: count, destination: destination.path, sourceIsSigned: plan.sourceIsSigned
                    )
                case let .failure(failure):
                    self.store.editStatus = failure.localizedDescription
                }
            }
        }
    }
}
