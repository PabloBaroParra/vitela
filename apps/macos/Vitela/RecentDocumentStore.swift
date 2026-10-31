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
    /// What lets this app read `url` again after a relaunch, made while it
    /// still may; `nil` when no bookmark is kept or none could be made.
    func bookmark(for url: URL) -> Data?
    /// Runs `body` with `document`'s file readable, then gives that access
    /// back.
    func withAccess<T>(to document: RecentDocument, _ body: (URL) throws -> T) rethrows -> T
}

extension RecentDocumentStore {
    func noteOpened(_ url: URL) {}
    func noteCleared() {}
    func bookmark(for url: URL) -> Data? { nil }
    func withAccess<T>(to document: RecentDocument, _ body: (URL) throws -> T) rethrows -> T {
        try body(document.url)
    }

    /// Whether the card's file is still there to open.
    func canReach(_ document: RecentDocument) -> Bool {
        withAccess(to: document) { FileManager.default.fileExists(atPath: $0.path) }
    }
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
///
/// In the App Sandbox (the Mac App Store build) a path alone opens nothing
/// after a relaunch: the access the open panel, a drop or Finder granted ends
/// with the process. So each entry keeps a security-scoped bookmark, and a
/// card's file is read inside that bookmark's access. Outside the sandbox the
/// bookmark resolves to the same file and changes nothing.
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

    func bookmark(for url: URL) -> Data? {
        try? url.bookmarkData(options: .withSecurityScope, includingResourceValuesForKeys: nil, relativeTo: nil)
    }

    /// The card's own path is what is read: a bookmark that now resolves to
    /// a file moved elsewhere grants access to that file, not to the card.
    func withAccess<T>(to document: RecentDocument, _ body: (URL) throws -> T) rethrows -> T {
        var stale = false
        guard let bookmark = document.bookmark,
              let scoped = try? URL(resolvingBookmarkData: bookmark, options: .withSecurityScope,
                                    relativeTo: nil, bookmarkDataIsStale: &stale)
        else { return try body(document.url) }
        let granted = scoped.startAccessingSecurityScopedResource()
        defer { if granted { scoped.stopAccessingSecurityScopedResource() } }
        return try body(document.url)
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
