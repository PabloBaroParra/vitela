// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Organize's Documents view: the pages grouped into one block per run from
/// the same PDF, each turned, deleted or moved as a whole — Linux's
/// `organize/documents`, Windows' `MainWindow.OrganizeDocuments.cs`.
///
/// The grouping is the core's (`document_blocks`); what stays here is what a
/// block is called and turning a card's action into one page edit. Blocks
/// are positions, current only until the next edit, so they are read again
/// whenever page work lands — and an edit can only start while none is in
/// flight, so the blocks it is built from are the ones on screen.
extension ViewerStore {
    /// What a Documents card's buttons do to its block.
    enum BlockAction: Equatable {
        case rotateLeft, rotateRight, delete
        /// To a drop slot, `0` before the first card — see `DocumentBlocks.move`.
        case move(toSlot: Int)
    }

    /// Re-reads the blocks; called whenever the layout may have changed.
    func refreshDocumentBlocks() {
        documentBlocks = document.map { client.documentBlocks(document: $0) } ?? []
    }

    /// Called by `resetEditing` once a document has replaced the previous one.
    func resetDocumentBlocks() {
        importedSourceNames = [:]
        refreshDocumentBlocks()
    }

    /// Every card, named: the open file by `baseName`, an import by the file
    /// picked for it, blank pages as such — Linux's `source_name`.
    func documentBlockRows(baseName: String) -> [DocumentBlockRow] {
        documentBlocks.map { block in
            let name: String
            switch block.source {
            case .base: name = baseName
            case .blank: name = DocumentBlocks.blankName
            case let .imported(id): name = importedSourceNames[id] ?? DocumentBlocks.importedName
            }
            return DocumentBlockRow(block: block, name: name)
        }
    }

    /// What each Pages card says about where its page came from — Linux's
    /// `source_names`: nothing while every page shares one source (naming it
    /// on every card is noise), and every card's source, the opened file's
    /// included, once there is more than one.
    func pageSourceNames(baseName: String) -> [String?] {
        let rows = documentBlockRows(baseName: baseName)
        let mixed = Set(rows.map(\.block.source)).count > 1
        return rows.flatMap { row -> [String?] in
            let name = row.block.source == .blank ? DocumentBlocks.blankPageName : row.name
            return Array(repeating: mixed ? name : nil, count: row.block.count)
        }
    }

    /// Starts `action` on block `position`, or says why not.
    func beginBlockEdit(_ position: Int, _ action: BlockAction) -> PageWorkRequest? {
        guard documentBlocks.indices.contains(position) else { return nil }
        let block = documentBlocks[position]
        switch action {
        case .rotateLeft: return beginPageEdit(.rotateBlock(from: block.start, count: block.count, degrees: -90))
        case .rotateRight: return beginPageEdit(.rotateBlock(from: block.start, count: block.count, degrees: 90))
        case .delete: return beginPageEdit(.removeBlock(from: block.start, count: block.count))
        case let .move(slot):
            guard let edit = DocumentBlocks.move(position, toSlot: slot, in: documentBlocks) else { return nil }
            return beginPageEdit(edit)
        }
    }
}
