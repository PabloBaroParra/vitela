// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
//
// Value types of the document operations — export, split, compress, page
// edits, imports — that `PdfCoreClient` takes and returns.
import Foundation

/// An exported page's encoding — mirrors `FfiExportFormat`. JPEG has no
/// alpha: the core flattens the page onto an opaque background.
enum ImageFormat: Equatable, CaseIterable {
    case png, jpeg

    var name: String { self == .png ? "PNG" : "JPEG" }
}

/// One file a split writes: pages `first...last` (zero-based) — mirrors
/// `FfiSplitPart`.
struct SplitPart: Equatable {
    let first: Int
    let last: Int
    let fileName: String
}

/// The compression the reader picked — mirrors `FfiCompressPreset`.
enum CompressPreset: CaseIterable, Equatable {
    case lossless, balanced, small
}

/// A finished compression — mirrors `FfiCompressedSave` and its report.
/// The core guarantees the bytes are never larger, not that they are smaller.
struct CompressionResult: Equatable {
    let bytes: Data
    let beforeBytes: Int
    let afterBytes: Int
    let savedBytes: Int
    let reduced: Bool
    /// What could not be done, as sentences.
    let refusals: [String]
}

/// One page-structure edit — the subset of `FfiEditCommand` the Organize
/// view issues, in page positions (0-based) of the document as it is now.
enum PageEdit: Equatable {
    enum Orientation: Equatable {
        case portrait, landscape
    }

    /// Turns a page by a multiple of 90°; positive is clockwise.
    case rotate(page: Int, degrees: Int)
    case remove(page: Int)
    /// An A4 blank page, as on Windows and Linux, inserted before `at`
    /// (`at == pageCount` appends).
    case insertBlank(at: Int, orientation: Orientation)
    /// Moves one page so it ends up at position `to`.
    case move(from: Int, to: Int)
    /// One Documents block — `count` pages from `from` — turned, removed or
    /// moved so it starts at `to`, each as one undo step
    /// (`PdfCore+Blocks.swift`).
    case rotateBlock(from: Int, count: Int, degrees: Int)
    case removeBlock(from: Int, count: Int)
    case moveBlock(from: Int, count: Int, to: Int)
}

extension PageEdit {
    /// Why this edit cannot apply to a document of `pageCount` pages, or
    /// `nil` when it can. A one-page edit is checked as a block of one.
    func refusal(pageCount: Int) -> String? {
        let invalid = "Invalid page position."
        func fits(_ from: Int, _ pages: Int) -> Bool { pages > 0 && from >= 0 && from + pages <= pageCount }
        // A turn by a whole number of revolutions changes nothing and would
        // still cost an undo step.
        func turns(_ degrees: Int) -> Bool { degrees % 90 == 0 && degrees % 360 != 0 }
        func removal(_ from: Int, _ pages: Int) -> String? {
            guard fits(from, pages) else { return invalid }
            return pages < pageCount ? nil : "A document needs at least one page."
        }
        switch self {
        case let .insertBlank(at, _):
            return at >= 0 && at <= pageCount ? nil : invalid
        case let .rotate(page, degrees):
            return fits(page, 1) && turns(degrees) ? nil : invalid
        case let .rotateBlock(from, pages, degrees):
            return fits(from, pages) && turns(degrees) ? nil : invalid
        case let .remove(page):
            return removal(page, 1)
        case let .removeBlock(from, pages):
            return removal(from, pages)
        case let .move(from, to):
            return fits(from, 1) && fits(to, 1) && from != to ? nil : invalid
        case let .moveBlock(from, pages, to):
            return fits(from, pages) && fits(to, pages) && from != to ? nil : invalid
        }
    }

    /// What the status line says once the core took it.
    var done: String {
        switch self {
        case .rotate: return "Page rotated."
        case .remove: return "Page deleted."
        case .insertBlank: return "Blank page added."
        case .move: return "Page moved."
        case .rotateBlock: return "Document rotated."
        case .removeBlock: return "Document deleted."
        case .moveBlock: return "Document moved."
        }
    }
}

/// Another PDF opened as a source of pages to add — mirrors the generated
/// `PreparedImport`. Preparing one changes nothing; importing spends it.
protocol ImportSource {
    /// Every page it has, which is what an import adds.
    var pageCount: Int { get }
    /// What its pages will not bring across exactly as they are, known before
    /// anything is added so the shell can ask first.
    var warnings: [String] { get }
}
