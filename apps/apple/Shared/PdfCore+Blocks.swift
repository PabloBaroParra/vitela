// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Which PDF a block's pages come from — mirrors `FfiBlockSource`.
enum BlockSource: Equatable {
    /// The PDF the document was opened from.
    case base
    /// Pages with no source PDF at all — inserted blank pages.
    case blank
    /// A PDF Add PDFs… brought in; `id` is the one its import reported.
    case imported(id: UInt64)
}

/// One contiguous run of pages from the same PDF — mirrors
/// `FfiDocumentBlock`, which is `pdf_document::derive_blocks`, the grouping
/// Linux's and Windows' Documents views read. Positions, not page ids: only
/// current until the next edit, so the store reads them again after each.
struct DocumentBlock: Equatable {
    let source: BlockSource
    /// 1-based, only when `source` is split across more than one block.
    let part: Int?
    /// Position of the block's first page.
    let start: Int
    /// Never zero.
    let count: Int
}

/// What an import added — mirrors `FfiBatchImportReport`.
struct ImportedBatch: Equatable {
    let pageCount: Int
    /// Each source's block id, in the order the sources were given.
    let sourceIds: [UInt64]
}

/// One Documents card, flattened: the block and what it is called.
struct DocumentBlockRow: Equatable {
    let block: DocumentBlock
    let name: String

    /// "report.pdf" or "report.pdf — Part 2" — Linux's `Row::title`.
    var title: String {
        block.part.map { "\(name) — Part \($0)" } ?? name
    }

    /// "7 pages · 3–9", or "1 page · 4" — where the block sits now.
    var meta: String {
        let first = block.start + 1
        return block.count == 1 ? "1 page · \(first)" : "\(block.count) pages · \(first)–\(block.start + block.count)"
    }
}

/// The Documents view's rules and wording — Linux's `organize::documents`,
/// Windows' `MainWindow.OrganizeDocuments.cs`.
enum DocumentBlocks {
    static let documentsHint = "Drag a document to move all of its pages."
    static let pagesHint = "Drag a page to reorder it."
    static let blankName = "Blank pages"
    /// A block whose import this session did not see — Linux's fallback.
    static let importedName = "Imported PDF"

    /// The move that puts block `position` at drop slot `slot` — one of the
    /// `blocks.count + 1` gaps around the cards, `0` before the first — as
    /// Windows' `ResolveBlockMove` computes it: `to` is where the block
    /// starts after the move. `nil` when the block would stay where it is.
    static func move(_ position: Int, toSlot slot: Int, in blocks: [DocumentBlock]) -> PageEdit? {
        guard blocks.indices.contains(position), (0...blocks.count).contains(slot),
              slot != position, slot != position + 1
        else { return nil }
        let block = blocks[position]
        let before = blocks.prefix(slot).reduce(0) { $0 + $1.count }
        return .moveBlock(from: block.start, count: block.count, to: slot > position ? before - block.count : before)
    }
}

/// Defaults for clients without the core's block grouping — kept beside the
/// block types rather than in `PdfCore.swift`'s general defaults.
extension PdfCoreClient {
    func documentBlocks(document: any PdfDocument) -> [DocumentBlock] { [] }
}
