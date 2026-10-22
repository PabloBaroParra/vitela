import AppKit
import Foundation
import UniformTypeIdentifiers

/// The questions Add PDFs asks, as closures so tests can answer them without
/// a window. `.modal` is the real AppKit implementation.
struct ImportPrompts {
    /// The PDFs to add, in the order picked; empty when cancelled.
    var choosePdfs: () -> [URL]
    /// The password for `name`, or `nil` to cancel the whole import.
    var askPassword: (_ name: String, _ retry: Bool) -> String?
    /// Whether to go ahead despite what the sources will not bring across.
    var confirmWarnings: (_ warnings: [String]) -> Bool

    static let modal = ImportPrompts(
        choosePdfs: {
            let panel = NSOpenPanel()
            panel.allowedContentTypes = [.pdf]
            panel.allowsMultipleSelection = true
            panel.prompt = "Add"
            return panel.runModal() == .OK ? panel.urls : []
        },
        askPassword: { name, retry in
            let alert = NSAlert()
            alert.messageText = "\(name) is password protected"
            alert.informativeText = retry ? "The password is incorrect. Try again." : "Enter its password to add its pages."
            let field = NSSecureTextField(frame: NSRect(x: 0, y: 0, width: 240, height: 24))
            alert.accessoryView = field
            alert.addButton(withTitle: "Add")
            alert.addButton(withTitle: "Cancel")
            alert.window.initialFirstResponder = field
            return alert.runModal() == .alertFirstButtonReturn ? field.stringValue : nil
        },
        confirmWarnings: { warnings in
            let alert = NSAlert()
            alert.alertStyle = .warning
            alert.messageText = "Some pages will not come across exactly"
            alert.informativeText = warnings.joined(separator: "\n")
            alert.addButton(withTitle: "Add Anyway")
            alert.addButton(withTitle: "Cancel")
            return alert.runModal() == .alertFirstButtonReturn
        }
    )
}

/// How far preparing a pick has got: `completed` of `total` PDFs are ready.
struct ImportProgress: Equatable {
    var completed: Int
    var total: Int

    var text: String { "\(completed) of \(total) PDFs" }
}

/// Organize ▸ Add PDFs: the macOS chain around `ViewerStore+Import.swift`.
/// Each source is read and prepared on the queue, one at a time; questions
/// come back to main. Nothing changes until every source is ready and any
/// warnings are accepted — then all of them are appended as one undo step.
/// Cancel import drops the pick while it is still being prepared, as Linux's
/// `organize/import.rs` and Windows' `CancelImportButton` do; once the pages
/// are being appended it is one core command and can no longer be cancelled.
extension ViewerViewModel {
    static let importCancelled = "PDF import cancelled."

    func importPdfs() {
        guard isOrganizing, !isImporting, store.canEditPages else { return }
        if let refusal = store.importRefusal() {
            store.editStatus = refusal
            return
        }
        let urls = importPrompts.choosePdfs()
        guard !urls.isEmpty else {
            store.editStatus = Self.importCancelled
            return
        }
        isImporting = true
        importRun += 1
        importProgress = ImportProgress(completed: 0, total: urls.count)
        store.editStatus = "Checking selected PDFs…"
        prepareSource(at: 0, of: urls, generation: store.generation, run: importRun, prepared: [], password: nil)
    }

    /// Drops the pick being prepared. Nothing has changed yet, so there is
    /// nothing to undo; a source still being read is discarded when it lands.
    func cancelImport() {
        guard isImporting else { return }
        importRun += 1
        endImport(Self.importCancelled)
    }

    private func prepareSource(at index: Int, of urls: [URL], generation: UInt, run: Int, prepared: [any ImportSource], password: String?) {
        guard index < urls.count else { return finishImport(prepared, from: urls, generation: generation, run: run) }
        let url = urls[index]
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let bytes = try? Data(contentsOf: url)
            let result = bytes.map { self.store.prepareImport(bytes: $0, password: password) }
            DispatchQueue.main.async {
                guard run == self.importRun else { return }
                guard self.importIsCurrent(generation) else { return self.endImport(Self.importCancelled) }
                let name = url.lastPathComponent
                switch result {
                case nil:
                    self.endImport("Could not import \(name): the file could not be read.")
                case let .success(source)?:
                    self.importProgress = ImportProgress(completed: index + 1, total: urls.count)
                    self.prepareSource(at: index + 1, of: urls, generation: generation, run: run, prepared: prepared + [source], password: nil)
                case let .failure(error)?:
                    switch error {
                    case ViewerFailure.passwordRequired, ViewerFailure.wrongPassword:
                        guard let typed = self.importPrompts.askPassword(name, password != nil) else {
                            return self.endImport(Self.importCancelled)
                        }
                        self.prepareSource(at: index, of: urls, generation: generation, run: run, prepared: prepared, password: typed)
                    default:
                        self.endImport("Could not import \(name): \(ViewerStore.editMessage(for: error))")
                    }
                }
            }
        }
    }

    private func finishImport(_ sources: [any ImportSource], from urls: [URL], generation: UInt, run: Int) {
        let warnings = zip(urls, sources).flatMap { url, source in
            source.warnings.map { "\(url.lastPathComponent): \($0)" }
        }
        guard warnings.isEmpty || importPrompts.confirmWarnings(warnings) else { return endImport(Self.importCancelled) }
        guard run == importRun, importIsCurrent(generation), let request = store.beginImport(sources, names: urls.map(\.lastPathComponent)) else {
            return endImport(Self.importCancelled)
        }
        isImporting = false
        importProgress = nil
        store.editStatus = "Adding selected PDFs…"
        enqueuePageWork(request)
    }

    /// The document an import was picked for is still the one on screen, in
    /// the layout it was picked against, and Organize is still open.
    private func importIsCurrent(_ generation: UInt) -> Bool {
        isOrganizing && store.generation == generation
    }

    private func endImport(_ status: String) {
        isImporting = false
        importProgress = nil
        store.editStatus = status
    }
}
