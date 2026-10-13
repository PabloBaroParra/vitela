import AppKit
import Foundation

/// Where Home's Recent list is kept. Every method touches storage, so callers
/// run them off the main thread.
protocol RecentDocumentStore {
    func load() -> [RecentDocument]
    /// `false` when the list could not be written (the old one is kept).
    func save(_ documents: [RecentDocument]) -> Bool
    /// Tells the system a document was opened. Main thread.
    func noteOpened(_ url: URL)
    /// Tells the system the list was cleared. Main thread.
    func noteCleared()
}

extension RecentDocumentStore {
    func noteOpened(_ url: URL) {}
    func noteCleared() {}
}

/// Vitela's own list, one JSON file under Application Support — local to
/// this Mac and this user, written atomically, readable only by its owner
/// because it names the user's files.
///
/// Linux and Windows read the desktop's shared recent-files store and keep a
/// separate hidden list for "Remove from Recent". macOS exposes no shared
/// store with open dates to read, so the list is Vitela's own — which also
/// means removing a card simply removes it, with no second list. Each open is
/// still noted with `NSDocumentController`, so the Dock menu lists it too.
struct FileRecentDocumentStore: RecentDocumentStore {
    let url: URL

    static var defaultURL: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Vitela", isDirectory: true)
            .appendingPathComponent("recent-documents.json")
    }

    /// The Dock menu's recent documents.
    func noteOpened(_ url: URL) {
        NSDocumentController.shared.noteNewRecentDocumentURL(url)
    }

    func noteCleared() {
        NSDocumentController.shared.clearRecentDocuments(nil)
    }

    func load() -> [RecentDocument] {
        guard let bytes = try? Data(contentsOf: url),
              let documents = try? JSONDecoder().decode([RecentDocument].self, from: bytes)
        else { return [] }
        return documents
    }

    func save(_ documents: [RecentDocument]) -> Bool {
        do {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try JSONEncoder().encode(documents).write(to: url, options: .atomic)
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
            return true
        } catch {
            return false
        }
    }
}

/// A list that lives only as long as the view model — the default, so a view
/// model made without the app's own store (every test) never touches the
/// reader's real Recent list or the Dock's.
final class InMemoryRecentDocumentStore: RecentDocumentStore {
    private let lock = NSLock()
    private var documents: [RecentDocument]

    init(_ documents: [RecentDocument] = []) {
        self.documents = documents
    }

    func load() -> [RecentDocument] {
        lock.lock()
        defer { lock.unlock() }
        return documents
    }

    func save(_ documents: [RecentDocument]) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        self.documents = documents
        return true
    }
}
