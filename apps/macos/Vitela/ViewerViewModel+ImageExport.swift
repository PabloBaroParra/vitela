import Foundation

/// File ▸ Export Pages as Images…: the macOS chain around
/// `ViewerStore+ImageExport.swift`. Mirrors Windows'
/// `MainWindow.ExportImages.cs` — choices first (checked while the sheet is
/// open), then a folder, then the files, one page at a time, stopping at the
/// first that fails and saying how many were written.
extension ViewerViewModel {
    func requestExportImages() {
        guard store.document != nil, !isSaving else { return }
        isExportImagesPresented = true
    }

    func cancelExportImages() {
        isExportImagesPresented = false
        store.editStatus = "Export cancelled."
    }

    /// Plans the sheet's choices. Returns the refusal to show, or `nil` once
    /// the sheet can close and the folder panel take over.
    func exportImages(pages: ViewerStore.ImageExportPages, dpi: Int, format: ImageFormat) -> String? {
        switch store.planImageExport(pages: pages, dpi: dpi, format: format, documentName: documentName ?? "document.pdf") {
        case let .failure(failure):
            return failure.localizedDescription
        case let .success(plan):
            isExportImagesPresented = false
            DispatchQueue.main.async { [weak self] in self?.writeImages(plan) }
            return nil
        }
    }

    private func writeImages(_ plan: ViewerStore.ImageExportPlan) {
        guard !isSaving else { return }
        guard let folder = prompts.chooseFolder() else {
            store.editStatus = "Export cancelled. No file was written."
            return
        }
        isSaving = true
        writeImage(0, of: plan, into: folder)
    }

    private func writeImage(_ index: Int, of plan: ViewerStore.ImageExportPlan, into folder: URL) {
        guard index < plan.files.count else {
            isSaving = false
            store.editStatus = ViewerStore.imageExportSummary(count: index, format: plan.format, folder: folder.path)
            return
        }
        let file = plan.files[index]
        guard let request = store.beginImageExport(file, of: plan) else {
            isSaving = false
            store.editStatus = "Export stopped after \(index) written: The document is no longer open."
            return
        }
        store.editStatus = "Exporting page \(file.page + 1) (\(index + 1) of \(plan.files.count))…"
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            // `nil` when the page landed; otherwise why the export stops.
            let stop: String?
            switch self.store.imageExportResult(for: request) {
            case let .failure(failure):
                stop = "Page \(file.page + 1) could not be exported: \(failure.localizedDescription) \(index) written."
            case let .success(bytes):
                do {
                    // Never overwrites, as on Windows: an earlier export into
                    // the same folder is the reader's, and a unique name
                    // costs nothing. `.atomic` keeps a failed write from
                    // leaving a partial file behind.
                    try bytes.write(to: Self.uniqueDestination(for: file.fileName, in: folder), options: .atomic)
                    stop = nil
                } catch {
                    stop = "Export stopped after \(index) written: \(error.localizedDescription)"
                }
            }
            DispatchQueue.main.async {
                guard let stop else { return self.writeImage(index + 1, of: plan, into: folder) }
                self.isSaving = false
                self.store.editStatus = stop
            }
        }
    }

    /// `name` in `folder`, or the first free "stem (n).ext" beside it — the
    /// name Windows' `GenerateUniqueName` would pick.
    static func uniqueDestination(for name: String, in folder: URL) -> URL {
        let first = folder.appendingPathComponent(name)
        guard FileManager.default.fileExists(atPath: first.path) else { return first }
        let stem = (name as NSString).deletingPathExtension
        let ext = (name as NSString).pathExtension
        for number in 2... {
            let candidate = folder.appendingPathComponent(ext.isEmpty ? "\(stem) (\(number))" : "\(stem) (\(number)).\(ext)")
            if !FileManager.default.fileExists(atPath: candidate.path) { return candidate }
        }
        return first
    }
}
