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

/// `search`/`pageCharacters` read the document's text, so they answer to the
/// same extraction permission a copy does.
enum TextQueryFailure: Error, Equatable {
    /// The document forbids text extraction.
    case notPermitted
    case failed(String)
}

extension TextQueryFailure: LocalizedError {
    var errorDescription: String? {
        switch self {
        case .notPermitted: return "Text extraction is not permitted for this document."
        case let .failed(message): return message
        }
    }
}

/// Renders are issued from a background queue, so a conforming client must be
/// safe to call from any thread. `UniFfiPdfCoreClient` is: it holds no mutable
/// state and every handle it hands out is `Arc`/`Mutex`-guarded on the Rust side.
protocol PdfCoreClient {
    func open(bytes: Data, password: String?) throws -> any PdfDocument
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
}

/// An edit or save the core refused, in words a user can act on.
enum EditFailure: Error, Equatable {
    /// The document's permissions forbid this kind of change.
    case notPermitted(String)
    /// The save would break a signature and the user has not agreed to that.
    case signaturesWouldBeInvalidated
    case failed(String)
}

extension EditFailure: LocalizedError {
    var errorDescription: String? {
        switch self {
        case let .notPermitted(message): return message
        case .signaturesWouldBeInvalidated: return "Saving would break this document's signature."
        case let .failed(message): return message
        }
    }
}

enum ViewerFailure: Error, Equatable {
    /// The bytes never reached the core — the file could not be read from disk.
    case readFailed(String)
    case openFailed(String)
    /// The document is encrypted and no password (or an incomplete one) was
    /// supplied — distinct from `wrongPassword` so the prompt can tell a
    /// first ask apart from a retry.
    case passwordRequired
    /// The supplied password matched neither the user nor owner password.
    case wrongPassword
    case renderFailed(page: Int, message: String)
    case invalidImage(page: Int)
}

extension ViewerFailure: LocalizedError {
    var errorDescription: String? { message }

    /// Deliberately not named `localizedDescription`: that would shadow the
    /// `Error` extension and silently change meaning at each call site.
    var message: String {
        switch self {
        case let .readFailed(message): return message
        case let .openFailed(message): return message
        case .passwordRequired: return "This document requires a password."
        case .wrongPassword: return "The password is incorrect. Try again."
        case let .renderFailed(_, message): return message
        case let .invalidImage(page): return "Page \(page + 1) returned invalid image data."
        }
    }
}
