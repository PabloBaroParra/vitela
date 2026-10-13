import Foundation

/// A Recent card's picture of page 1 and its page count, once rendered.
struct RecentPreview: Equatable {
    let image: RenderedPage?
    let pageCount: Int
}

/// Home's Recent list: every PDF opened from a file is recorded once it has
/// loaded, shown newest first on the start screen and under File ▸ Open
/// Recent, and can be removed. Mirrors Windows' `MainWindow.Recents.cs` and
/// Linux's `home/recents.rs`; the rules are `RecentDocuments` (shared), the
/// storage `RecentDocumentStore`.
extension ViewerViewModel {
    /// Windows' preview resolution: a 24 DPI page 1, scaled for the display.
    static func recentPreviewDpi(scale: Double) -> Int {
        Int(min(max((24 * max(scale, 1)).rounded(.up), 24), 96))
    }

    /// Reads the stored list off the main thread, dropping documents whose
    /// file is gone — a card that cannot open is only noise.
    func loadRecentDocuments() {
        let store = recentStore
        recentStorageQueue.addOperation { [weak self] in
            let documents = store.load().filter { FileManager.default.fileExists(atPath: $0.path) }
            DispatchQueue.main.async { self?.recentDocuments = documents }
        }
    }

    /// Records the file an open came from once its document has loaded. A
    /// password prompt keeps it pending; any other outcome forgets it.
    func recordPendingRecent(_ state: ViewerState) {
        guard let path = pendingRecentPath else { return }
        switch state {
        case .loaded:
            pendingRecentPath = nil
            recordRecent(path)
        case .loading, .error(.passwordRequired), .error(.wrongPassword):
            break
        case .empty, .error:
            pendingRecentPath = nil
        }
    }

    /// Records `path` as opened at `date`.
    func recordRecent(_ path: String, at date: Date = Date()) {
        // Reopened, perhaps changed: its card draws page 1 again.
        let standardized = URL(fileURLWithPath: path).standardizedFileURL.path
        recentPreviews[standardized] = nil
        recentPreviewsRequested.remove(standardized)
        recentStore.noteOpened(URL(fileURLWithPath: path))
        changeRecents(failure: nil) { RecentDocuments.recording(path, at: date, in: $0) }
    }

    /// "Remove from Recent": off this list only.
    func removeRecent(_ document: RecentDocument) {
        recentDocuments = RecentDocuments.removing(document.path, from: recentDocuments)
        changeRecents(failure: "Could not remove \(document.name) from Recent.") {
            RecentDocuments.removing(document.path, from: $0)
        }
    }

    /// File ▸ Open Recent ▸ Clear Menu: empties Vitela's list.
    func clearRecents() {
        recentDocuments = []
        recentStore.noteCleared()
        changeRecents(failure: "Could not clear Recent.") { _ in [] }
    }

    /// Opens a Recent card's document, after the unsaved-changes question.
    func openRecent(_ document: RecentDocument) {
        proceedPastUnsavedChanges { [weak self] in
            self?.open(url: document.url)
        }
    }

    /// Renders page 1 of each card still without a picture, one document at
    /// a time on the operation queue, so a reader's own open is never stuck
    /// behind a row of previews. A file that will not open — a password, a
    /// damaged file — keeps a blank frame, as on Windows.
    ///
    /// `retry` asks again for previews queued earlier: an open cancels the
    /// operation queue, so Home coming back on screen retries them.
    func loadRecentPreviews(scale: Double, retry: Bool = false) {
        let client = store.client
        let dpi = Self.recentPreviewDpi(scale: scale)
        if retry { recentPreviewsRequested.removeAll() }
        for document in recentDocuments
        where recentPreviews[document.path] == nil && !recentPreviewsRequested.contains(document.path) {
            recentPreviewsRequested.insert(document.path)
            operationQueue.addOperation { [weak self] in
                guard let bytes = try? Data(contentsOf: document.url),
                      let opened = try? client.open(bytes: bytes, password: nil),
                      !opened.pages.isEmpty
                else { return }
                let image = try? client.render(document: opened, page: 0, dpi: dpi)
                let preview = RecentPreview(image: image, pageCount: opened.pages.count)
                DispatchQueue.main.async { self?.recentPreviews[document.path] = preview }
            }
        }
    }

    /// Applies `change` to the stored list on the storage queue, so it
    /// always starts from what is on disk — never from a copy the first load
    /// has not filled in yet — then shows the result.
    private func changeRecents(failure: String?, _ change: @escaping ([RecentDocument]) -> [RecentDocument]) {
        let store = recentStore
        recentStorageQueue.addOperation { [weak self] in
            let documents = change(store.load())
            let saved = store.save(documents)
            let shown = documents.filter { FileManager.default.fileExists(atPath: $0.path) }
            DispatchQueue.main.async {
                guard let self else { return }
                self.recentDocuments = shown
                if !saved, let failure { self.store.editStatus = failure }
            }
        }
    }
}
