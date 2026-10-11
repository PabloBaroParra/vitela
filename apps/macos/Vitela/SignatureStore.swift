import Foundation

/// The one drawn signature the reader asked to be remembered on this Mac, as
/// the PNG the pad made — Windows' `ISignatureStore`. Every method touches
/// storage, so callers run them off the main thread.
protocol SignatureStore {
    /// The remembered PNG, or `nil` when there is none or it cannot be read.
    func load() -> Data?
    /// Replaces the remembered PNG; `false` when it could not be written (the
    /// old one is kept).
    func save(_ png: Data) -> Bool
    /// Forgets it. Nothing to forget is not an error.
    func delete()
}

/// The signature kept in one file under Application Support — local to this
/// Mac and this user, never synced, as Windows keeps it under LOCALAPPDATA.
/// Written atomically, so a crash mid-write leaves the old signature, not
/// half of the new one.
struct FileSignatureStore: SignatureStore {
    let url: URL

    static var defaultURL: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Vitela", isDirectory: true)
            .appendingPathComponent("signature.png")
    }

    func load() -> Data? {
        guard let bytes = try? Data(contentsOf: url), !bytes.isEmpty else { return nil }
        return bytes
    }

    func save(_ png: Data) -> Bool {
        do {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try png.write(to: url, options: .atomic)
            return true
        } catch {
            return false
        }
    }

    func delete() {
        try? FileManager.default.removeItem(at: url)
    }
}
