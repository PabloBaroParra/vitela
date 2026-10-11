// Shared by the macOS and iOS shells. Nothing here may import AppKit or UIKit:
// the moment it does, one of the two platforms stops compiling. Platform
// chrome (app entry, file picking, views) stays in apps/macos and apps/ios.
import Foundation

struct PageDimensions: Equatable {
    let width: Double
    let height: Double
}

struct RenderedPage: Equatable {
    let rgba: Data
    let width: Int
    let height: Int
    let stride: Int

    static let placeholder = RenderedPage(rgba: Data([0, 0, 0, 0]), width: 1, height: 1, stride: 4)
}

protocol PdfDocument {
    var pages: [PageDimensions] { get }
}

/// A `/CreationDate` or `/ModDate` value — mirrors `FfiPdfDate`. The offset
/// is carried along unchanged so an edited date keeps the zone it had.
struct MetadataDate: Equatable {
    enum Offset: Equatable {
        case utc
        case plus(hours: Int, minutes: Int)
        case minus(hours: Int, minutes: Int)
    }

    var year: Int
    var month: Int
    var day: Int
    var hour: Int
    var minute: Int
    var second: Int
    var offset: Offset
}

/// The document's Info dictionary — mirrors `FfiDocumentInfo`. `nil` means
/// the entry is absent; an empty text field clears it.
struct DocumentInfo: Equatable {
    var title: String?
    var author: String?
    var subject: String?
    var keywords: String?
    var creator: String?
    var producer: String?
    var creationDate: MetadataDate?
    var modDate: MetadataDate?
}

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

/// A page-space rectangle in PDF points with a bottom-left origin — mirrors
/// `FfiTextRect`. A shell converts to view space (top-left origin) with the
/// page's height: `y_view = (pageHeight - rect.yPt - rect.heightPt) * zoom`.
struct TextRect: Equatable {
    let xPt: Double
    let yPt: Double
    let widthPt: Double
    let heightPt: Double
}

struct SearchMatch: Equatable {
    let pageIndex: Int
    let text: String
    let characterBounds: [TextRect]
}

/// One page's characters, flattened for repeated caret hit-testing during a
/// drag-select. Obtained via `PdfCoreClient.pageCharacters` and cached by
/// `ViewerStore` for the life of the open document — see
/// `ViewerStore+Selection.swift`.
protocol PageCharacters {
    /// The caret nearest a PDF-space point (bottom-left origin), or `nil` on
    /// a page with no positioned text.
    func caretAt(xPt: Double, yPt: Double) -> Int?
    /// The text between two carets, for the clipboard. `anchor`/`focus` need
    /// not be ordered.
    func textIn(anchor: Int, focus: Int) -> String
    /// The rects a shell paints between two carets: one per visual line.
    func rectsIn(anchor: Int, focus: Int) -> [TextRect]
}

/// Renders are issued from a background queue, so a conforming client must be
/// safe to call from any thread. `UniFfiPdfCoreClient` is: it holds no mutable
/// state and every handle it hands out is `Arc`/`Mutex`-guarded on the Rust side.
protocol PdfCoreClient {
    func open(bytes: Data, password: String?) throws -> any PdfDocument
    /// Opens with both passwords of a file this shell protected.
    func open(bytes: Data, userPassword: String, ownerPassword: String) throws -> any PdfDocument
    /// A new document holding one blank portrait A4 page.
    func createBlankDocument() throws -> any PdfDocument
    func render(document: any PdfDocument, page: Int, dpi: Int) throws -> RenderedPage
    func search(document: any PdfDocument, query: String) throws -> [SearchMatch]
    func pageCharacters(document: any PdfDocument, page: Int) throws -> any PageCharacters

    // Editing — see `ViewerStore+Editing.swift`. Every edit is applied to the
    // open handle's in-memory model; nothing reaches disk until `save`.
    func documentInfo(document: any PdfDocument) throws -> DocumentInfo
    func setDocumentInfo(document: any PdfDocument, info: DocumentInfo) throws
    func contentEditingAllowed(document: any PdfDocument) -> Bool
    func undo(document: any PdfDocument) -> Bool
    func redo(document: any PdfDocument) -> Bool
    func canUndo(document: any PdfDocument) -> Bool
    func canRedo(document: any PdfDocument) -> Bool
    /// Whether saving would break a signature the file already carries.
    func saveWillInvalidateSignatures(document: any PdfDocument) throws -> Bool
    /// The complete saved PDF. `acknowledgingSignatureLoss` is how the shell
    /// says it already warned the user; without it a signature-breaking save
    /// is refused.
    func save(document: any PdfDocument, acknowledgingSignatureLoss: Bool) throws -> Data

    // Extracting pages — see `ViewerStore+Extract.swift`. Never changes the
    // open document.
    func textExtractionAllowed(document: any PdfDocument) -> Bool
    /// Whether the document survives a full rewrite with the credentials it
    /// was opened with — an extraction is one.
    func fullRewriteAllowed(document: any PdfDocument) -> Bool
    /// One-based typed pages (`"1-3,7"`) to ascending zero-based positions;
    /// throws `EditFailure.failed` with the core's own sentence.
    func parsePageSelection(_ text: String, totalPages: Int) throws -> [Int]
    /// A new PDF holding only `pages` of the document.
    func extractPages(document: any PdfDocument, pages: [Int]) throws -> Data
    func extractSourceIsSigned(document: any PdfDocument) -> Bool
    /// The parts typed cut points make, with safe file names from the
    /// document's — the Linux shell's rules, now the core's.
    func planSplit(_ cuts: String, totalPages: Int, documentName: String) throws -> [SplitPart]

    // Exporting pages as images — see `ViewerStore+ImageExport.swift`. Draws
    // what `render` draws; the encoder is the core's, shared with Linux and
    // Windows, so the same page comes out byte-identical everywhere.
    func exportPageImage(document: any PdfDocument, page: Int, dpi: Int, format: ImageFormat) throws -> Data
    /// The first of `pages` over the core's raster ceiling at `dpi`, if any.
    func firstPageTooLargeToExport(document: any PdfDocument, pages: [Int], dpi: Int) -> Int?
    /// `"report.pdf"` page 3 of 12 as JPEG → `"report-03.jpg"`; always one
    /// path component.
    func pageImageFileName(documentName: String, page: Int, totalPages: Int, format: ImageFormat) -> String

    // Compressing — see `ViewerStore+Compress.swift`. Never changes the open
    // document; the bytes are a smaller copy for the shell to write.
    /// Why this document cannot be compressed, as a sentence, or `nil`.
    func compressionRefusal(document: any PdfDocument) -> String?
    /// The compression's own signature question — not
    /// `saveWillInvalidateSignatures`, which asks about a different save.
    func compressionWillInvalidateSignatures(document: any PdfDocument) throws -> Bool
    func compress(document: any PdfDocument, preset: CompressPreset, acknowledgingSignatureLoss: Bool) throws -> CompressionResult

    // Digital signing — see `ViewerStore+Signing.swift`.
    /// Why the document cannot be signed, as a sentence, or `nil`.
    func signingRefusal(document: any PdfDocument) -> String?
    func openSigningCertificate(bytes: Data, password: String) throws -> any SigningSource
    /// The signed PDF; the open document is untouched.
    func sign(document: any PdfDocument, source: any SigningSource, identityId: String) throws -> Data
    /// Opens signed output with the open document's credentials.
    func reopenSigned(document: any PdfDocument, bytes: Data) throws -> any PdfDocument

    // Protecting — see `ViewerStore+Protect.swift`.
    func protectionWillInvalidateSignatures(document: any PdfDocument) throws -> Bool
    /// The complete document, encrypted: `openPassword` to open it,
    /// `permissionsPassword` to lift its restrictions.
    func protect(
        document: any PdfDocument, openPassword: String, permissionsPassword: String, acknowledgingSignatureLoss: Bool
    ) throws -> Data

    // Page structure — see `ViewerStore+Pages.swift`. Unlike a metadata edit,
    // a page edit changes what is drawn, so the render side has to be
    // re-derived (`refreshPreview`) before `pageDimensions` and renders show it.
    func applyPageEdit(document: any PdfDocument, edit: PageEdit) throws
    /// Rebuilds what `render` draws from the pending edits.
    func refreshPreview(document: any PdfDocument) throws
    /// The page sizes `render` currently draws, in page order.
    func pageDimensions(document: any PdfDocument) -> [PageDimensions]
}

/// Default, harmless implementations so existing/test conformers that predate
/// search/selection don't have to grow methods they never exercise.
extension PdfCoreClient {
    func search(document: any PdfDocument, query: String) throws -> [SearchMatch] { [] }

    func pageCharacters(document: any PdfDocument, page: Int) throws -> any PageCharacters {
        throw TextQueryFailure.failed("this client does not support text queries")
    }

    func documentInfo(document: any PdfDocument) throws -> DocumentInfo { DocumentInfo() }

    func setDocumentInfo(document: any PdfDocument, info: DocumentInfo) throws {
        throw EditFailure.failed("this client does not support editing")
    }

    func contentEditingAllowed(document: any PdfDocument) -> Bool { false }
    func undo(document: any PdfDocument) -> Bool { false }
    func redo(document: any PdfDocument) -> Bool { false }
    func canUndo(document: any PdfDocument) -> Bool { false }
    func canRedo(document: any PdfDocument) -> Bool { false }
    func saveWillInvalidateSignatures(document: any PdfDocument) throws -> Bool { false }

    func save(document: any PdfDocument, acknowledgingSignatureLoss: Bool) throws -> Data {
        throw EditFailure.failed("this client does not support saving")
    }

    func applyPageEdit(document: any PdfDocument, edit: PageEdit) throws {
        throw EditFailure.failed("this client does not support page editing")
    }

    func refreshPreview(document: any PdfDocument) throws {}

    func pageDimensions(document: any PdfDocument) -> [PageDimensions] { document.pages }

    func textExtractionAllowed(document: any PdfDocument) -> Bool { false }
    func fullRewriteAllowed(document: any PdfDocument) -> Bool { false }

    func parsePageSelection(_ text: String, totalPages: Int) throws -> [Int] {
        throw EditFailure.failed("this client does not support page selections")
    }

    func extractPages(document: any PdfDocument, pages: [Int]) throws -> Data {
        throw EditFailure.failed("this client does not support extracting pages")
    }

    func extractSourceIsSigned(document: any PdfDocument) -> Bool { false }

    func planSplit(_ cuts: String, totalPages: Int, documentName: String) throws -> [SplitPart] {
        throw EditFailure.failed("this client does not support splitting")
    }

    func exportPageImage(document: any PdfDocument, page: Int, dpi: Int, format: ImageFormat) throws -> Data {
        throw EditFailure.failed("this client does not support exporting images")
    }

    func firstPageTooLargeToExport(document: any PdfDocument, pages: [Int], dpi: Int) -> Int? { nil }

    func pageImageFileName(documentName: String, page: Int, totalPages: Int, format: ImageFormat) -> String {
        "page-\(page + 1).\(format == .png ? "png" : "jpg")"
    }

    func compressionRefusal(document: any PdfDocument) -> String? { "This client does not support compression." }
    func compressionWillInvalidateSignatures(document: any PdfDocument) throws -> Bool { false }

    func compress(document: any PdfDocument, preset: CompressPreset, acknowledgingSignatureLoss: Bool) throws -> CompressionResult {
        throw EditFailure.failed("this client does not support compression")
    }

    func signingRefusal(document: any PdfDocument) -> String? { "This client does not support signing." }

    func openSigningCertificate(bytes: Data, password: String) throws -> any SigningSource {
        throw SigningSourceFailure.failed("this client does not support signing")
    }

    func sign(document: any PdfDocument, source: any SigningSource, identityId: String) throws -> Data {
        throw EditFailure.failed("this client does not support signing")
    }

    func reopenSigned(document: any PdfDocument, bytes: Data) throws -> any PdfDocument {
        throw ViewerFailure.openFailed("this client does not support signing")
    }

    func open(bytes: Data, userPassword: String, ownerPassword: String) throws -> any PdfDocument {
        try open(bytes: bytes, password: ownerPassword)
    }

    func createBlankDocument() throws -> any PdfDocument {
        throw ViewerFailure.openFailed("this client cannot create documents")
    }

    func protectionWillInvalidateSignatures(document: any PdfDocument) throws -> Bool { false }

    func protect(
        document: any PdfDocument, openPassword: String, permissionsPassword: String, acknowledgingSignatureLoss: Bool
    ) throws -> Data {
        throw EditFailure.failed("this client does not support protection")
    }
}
