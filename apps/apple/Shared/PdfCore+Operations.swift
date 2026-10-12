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
