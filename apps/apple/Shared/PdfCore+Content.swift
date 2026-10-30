// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// How a run's font is stored — mirrors `FfiFontKind`.
enum ContentFontKind: Equatable {
    case standard14, embeddedSimple, embeddedComposite
}

/// One line of text the page itself paints, as the core parsed it — mirrors
/// `FfiContentTextRun`. Sent back unchanged with an edit, so the core can
/// find the run again and amend a pending retype of it.
struct ContentTextRun: Equatable, Identifiable {
    let id: UInt64
    let page: Int
    let bbox: PdfRect
    let resourceFontName: String
    let fontKind: ContentFontKind
    let text: String

    /// The core cannot re-encode a composite (CID) font, so retyping such a
    /// run swaps in a standard font — Windows' `RequiresFontSubstitution`.
    var substitutesFont: Bool { fontKind == .embeddedComposite }
}

/// Runs that read as one piece of text — on one line, in one font, no more
/// than a space apart (B25 T-207) — mirrors `FfiTextBlock`. The core groups
/// them; a run that joins nothing is a block of one, so every run is in
/// exactly one block. Windows' `ContentTextBlock`.
struct ContentTextBlock: Equatable, Identifiable {
    /// The member runs, left to right; never empty.
    let runs: [ContentTextRun]
    let bbox: PdfRect
    let text: String

    /// A block of one, for a run the core reported no block for.
    init(_ run: ContentTextRun) {
        self.init(runs: [run], bbox: run.bbox, text: run.text)
    }

    init(runs: [ContentTextRun], bbox: PdfRect, text: String) {
        precondition(!runs.isEmpty, "a text block has at least one run")
        self.runs = runs
        self.bbox = bbox
        self.text = text
    }

    /// The run whose font, id and editor a retype of the whole block uses.
    var first: ContentTextRun { runs[0] }
    var id: UInt64 { first.id }
    var page: Int { first.page }
    /// More than one run — the case the block commands exist for. A block
    /// of one keeps the single-run edits, exactly as before T-207.
    var isMultiRun: Bool { runs.count > 1 }
    var substitutesFont: Bool { first.substitutesFont }
}

/// One image the page paints — mirrors `FfiContentImageItem`.
struct ContentImage: Equatable {
    let id: UInt64
    let page: Int
    let bbox: PdfRect
    let resourceXObjectName: String?
}

/// How a vector path is painted — mirrors `FfiPathPaint`.
enum PathPaint: Equatable {
    case stroke, fill, fillAndStroke
}

/// One vector path the page paints — a table border, a rule, a shaded box
/// (B25 T-206) — mirrors `FfiContentPathItem`. A rule's box may be flat.
struct ContentPath: Equatable {
    let id: UInt64
    let page: Int
    let bbox: PdfRect
    let paint: PathPaint

    /// Only stroked: it is its edges, not the area inside — Windows' `IsOutline`.
    var isOutline: Bool { paint == .stroke }
}

/// One page's editable content — mirrors `FfiPageContent`, plus the paths
/// the core reads separately (`read_page_paths`).
struct PageContent: Equatable {
    let textRuns: [ContentTextRun]
    let images: [ContentImage]
    var paths: [ContentPath] = []
    /// The core's grouping of `textRuns` (`read_page_text_blocks`).
    var textBlocks: [ContentTextBlock] = []

    /// The block `run` belongs to, or a block of one when the core grouped
    /// none — so a shell without blocks behaves exactly as before.
    func block(containing run: ContentTextRun) -> ContentTextBlock {
        textBlocks.first { $0.runs.contains { $0.id == run.id } } ?? ContentTextBlock(run)
    }
}

/// A change to what a page paints — the content half of `FfiEditCommand`.
enum ContentEdit: Equatable {
    /// Retypes `run`; a composite-font run gets a standard font instead.
    case retypeText(ContentTextRun, to: String)
    case removeText(ContentTextRun)
    /// Moves `run` so its box lands at `to`, keeping its text, font and size.
    case moveText(ContentTextRun, to: PdfRect)
    /// A new line of Helvetica: `rect`'s height is its size, its top-left
    /// corner where the line starts.
    case insertText(page: Int, rect: PdfRect, text: String)
    case moveImage(ContentImage, to: PdfRect)
    /// A new box for the image; a corner drag may move its origin too.
    case resizeImage(ContentImage, to: PdfRect)
    case removeImage(ContentImage)
    /// A new picture in the image's box; the core keeps the old one for undo.
    case replaceImage(ContentImage, with: Data)
    /// A new image on `page` filling `rect`.
    case insertImage(page: Int, rect: PdfRect, bytes: Data)
    /// The same text or picture stamped on every page `Watermark.pages` names, as one
    /// undo step — `PdfCore+Watermark.swift`.
    case addWatermark(Watermark)
    /// Moves `path` so its box lands at `to`; the core rewrites its
    /// coordinates, so its line width does not change.
    case movePath(ContentPath, to: PdfRect)
    /// A new box for the path from a corner drag; a flat axis stays flat.
    case resizePath(ContentPath, to: PdfRect)
    case removePath(ContentPath)
    /// Moves every run of a multi-run block by `dx`, `dy` points, as one step.
    case moveTextBlock(ContentTextBlock, dx: Double, dy: Double)
    case removeTextBlock(ContentTextBlock)
    /// All of the text in the block's first run, the others removed, as one
    /// step; a composite first run is painted in a standard font instead.
    case retypeTextBlock(ContentTextBlock, to: String)
    /// Moves every member of a multi-selection by `dx`, `dy`, as one step.
    case moveSelection(ContentSelection, dx: Double, dy: Double)
    case removeSelection(ContentSelection)
    /// `clip` on `page` with its top-left corner at `left`, `top`, one step.
    case pasteContent(page: Int, clip: ContentClip, left: Double, top: Double)

    /// What the status line says once the core took it — the Windows and
    /// Android wording.
    var done: String {
        switch self {
        case .retypeText: return "Text updated."
        case .removeText: return "Text deleted."
        case .moveText: return "Text moved."
        case .insertText: return "Text inserted."
        case .moveImage: return "Image moved."
        case .resizeImage: return "Image resized."
        case .removeImage: return "Image deleted."
        case .replaceImage: return "Image replaced."
        case .insertImage: return "Image inserted."
        case let .addWatermark(watermark): return Watermarking.summary(count: watermark.pages.count)
        case .movePath: return "Shape moved."
        case .resizePath: return "Shape resized."
        case .removePath: return "Shape deleted."
        case .moveTextBlock: return "Text moved."
        case .removeTextBlock: return "Text deleted."
        case .retypeTextBlock: return "Text updated."
        case .moveSelection: return "Selection moved."
        case .removeSelection: return "Selection deleted."
        case .pasteContent: return "Pasted."
        }
    }
}

/// Defaults for clients without the core's content editor — kept beside the
/// content types rather than in `PdfCore.swift`'s general defaults.
extension PdfCoreClient {
    func readPageContent(document: any PdfDocument, page: Int) throws -> PageContent {
        PageContent(textRuns: [], images: [])
    }

    func applyContentEdit(document: any PdfDocument, edit: ContentEdit) throws {
        throw EditFailure.notPermitted(ContentEditing.forbidden)
    }

    func textRunHasPendingMove(document: any PdfDocument, run: ContentTextRun) -> Bool { false }

    func contentEditsCanCheckpoint(document: any PdfDocument) -> Bool { false }

    func copyContent(document: any PdfDocument, selection: ContentSelection) throws -> ContentClip {
        throw EditFailure.notPermitted(ContentEditing.copyRefused)
    }

    func imageReplacementRefusal(document: any PdfDocument, image: ContentImage) -> String? { ContentEditing.replaceRefused }
}
