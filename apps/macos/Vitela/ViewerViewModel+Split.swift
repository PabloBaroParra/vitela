import Foundation

/// File ▸ Split PDF…: the macOS chain around `ViewerStore+Split.swift`.
/// Mirrors Windows' `MainWindow.SplitPages.cs` — gates before the sheet, the
/// sheet stays open on a refusal, a folder is picked last, replacing existing
/// files is asked once for the whole set, and parts are written one at a time
/// through temporary files, stopping at the first that fails.
extension ViewerViewModel {
    /// Whether Split can be asked for now: a cut needs two pages. Shared by
    /// the File menu and the Organize header.
    var canRequestSplit: Bool { store.pageSlots.count >= 2 && !isSaving }

    func requestSplitPages() {
        guard store.document != nil, !isSaving else { return }
        if case let .failure(failure) = store.planSplit(cuts: "1", documentName: splitDocumentName) {
            store.editStatus = failure.localizedDescription
            return
        }
        isSplitPresented = true
    }

    func cancelSplitPages() {
        isSplitPresented = false
        store.editStatus = "Split cancelled."
    }

    /// Plans `cuts`. Returns the refusal for the sheet to show, or `nil`
    /// once the sheet can close and the folder panel take over.
    func splitPages(cuts: String) -> String? {
        switch store.planSplit(cuts: cuts, documentName: splitDocumentName) {
        case let .failure(failure):
            return failure.localizedDescription
        case let .success(plan):
            isSplitPresented = false
            DispatchQueue.main.async { [weak self] in self?.writeSplit(plan) }
            return nil
        }
    }

    private var splitDocumentName: String {
        documentName ?? Self.extractSuggestedName
    }

    private func writeSplit(_ plan: ViewerStore.SplitPlan) {
        guard !isSaving else { return }
        guard let folder = prompts.chooseFolder() else {
            store.editStatus = "Split cancelled. No file was written."
            return
        }
        let existing = plan.parts.filter {
            FileManager.default.fileExists(atPath: folder.appendingPathComponent($0.fileName).path)
        }.count
        guard existing == 0 || prompts.confirmReplace(existing, plan.parts.count) else {
            store.editStatus = "Split cancelled."
            return
        }
        isSaving = true
        writePart(0, of: plan, into: folder)
    }

    private func writePart(_ index: Int, of plan: ViewerStore.SplitPlan, into folder: URL) {
        guard index < plan.parts.count else {
            isSaving = false
            store.editStatus = ViewerStore.splitSummary(
                written: index, folder: folder.path, sourceIsSigned: plan.sourceIsSigned
            )
            return
        }
        let part = plan.parts[index]
        guard let request = store.beginSplitPart(part, of: plan) else {
            isSaving = false
            store.editStatus = ViewerStore.splitStopped(written: index, part: part.fileName, reason: "The document is no longer open.")
            return
        }
        store.editStatus = "Writing part \(index + 1) of \(plan.parts.count)…"
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let outcome = self.store.extractResult(for: request).flatMap { bytes -> Result<Void, EditFailure> in
                // `.atomic` writes a sibling temporary and renames it over
                // the part's name: only a complete part ever lands there.
                Result { try bytes.write(to: folder.appendingPathComponent(part.fileName), options: .atomic) }
                    .mapError { .failed($0.localizedDescription) }
            }
            DispatchQueue.main.async {
                switch outcome {
                case .success:
                    self.writePart(index + 1, of: plan, into: folder)
                case let .failure(failure):
                    self.isSaving = false
                    self.store.editStatus = ViewerStore.splitStopped(
                        written: index, part: part.fileName, reason: failure.localizedDescription
                    )
                }
            }
        }
    }
}
