// Adding other PDFs' pages to the open document — Organize ▸ Add PDFs.
// Mirrors Windows' `MainWindow.OrganizeImport.cs` and the Linux shell's
// `organize/import.rs`: ask the core whether this document can take pages at
// all before any file is picked, prepare each picked source without changing
// anything (asking for its password as often as it takes), then append every
// page of every source as ONE undo step through the page-work pipeline.
import Foundation

extension ViewerStore {
    /// Why the open document cannot take imported pages, or `nil` when it can.
    func importRefusal() -> String? {
        guard let document else { return "The document is no longer open." }
        return client.importRefusal(document: document)
    }

    /// Thread-safe: only the immutable client. Changes nothing.
    func prepareImport(bytes: Data, password: String?) -> Result<any ImportSource, Error> {
        Result { try client.prepareImport(bytes: bytes, password: password) }
    }

    /// Starts appending `sources`, or `nil` when there is nothing to add or
    /// other page work is running.
    func beginImport(_ sources: [any ImportSource]) -> PageWorkRequest? {
        guard let document, !pageWorkInFlight, !sources.isEmpty else { return nil }
        pageWorkInFlight = true
        return PageWorkRequest(generation: generation, document: document, work: .importSources(sources))
    }
}
