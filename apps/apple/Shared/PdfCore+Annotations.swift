// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
//
// Annotation value types — mirrors of the generated `FfiAnnotation*`,
// `FfiRect`/`FfiPoint`/`FfiColor` and page-placement types, so `ViewerStore`
// never sees a UniFFI type.
import Foundation

/// A rectangle in page space: PDF points, bottom-left origin, unrotated.
struct PdfRect: Equatable {
    var x: Double
    var y: Double
    var width: Double
    var height: Double

    func contains(_ point: PdfPoint) -> Bool {
        point.x >= x && point.x <= x + width && point.y >= y && point.y <= y + height
    }
}

struct PdfPoint: Equatable {
    var x: Double
    var y: Double
}

struct PdfColor: Equatable {
    let r: UInt8
    let g: UInt8
    let b: UInt8

    /// The color every shell gives a new annotation.
    static let annotationDefault = PdfColor(r: 255, g: 220, b: 0)
}

enum AnnotationKind: Equatable {
    case highlight, underline, strikeout, shape, ink, textNote, stamp, freeText

    /// Only kinds carrying a color can be restyled — Linux's
    /// `supports_restyle`, Windows' `SupportsRestyle`.
    var supportsRestyle: Bool {
        switch self {
        case .highlight, .underline, .strikeout, .ink, .shape: return true
        case .textNote, .stamp, .freeText: return false
        }
    }
}

/// One annotation this session added, as the core reports it. Annotations
/// the file already carried are drawn by the renderer and are not listed.
struct Annotation: Equatable {
    let id: UInt64
    let page: Int
    let kind: AnnotationKind
    /// `nil` for ink, which has only points.
    let rect: PdfRect?
    let points: [PdfPoint]
    let color: PdfColor?
    let contents: String?
    /// A text box's lines as the core laid them out; `nil` for other kinds.
    var freeTextLayout: FreeTextLayout? = nil

    /// The area it occupies, for hit-testing and handles: its rect, or the
    /// bounding box of an ink stroke's points — Windows' `AnnotationBounds`.
    var bounds: PdfRect? {
        if let rect { return rect }
        guard let first = points.first else { return nil }
        var minX = first.x, minY = first.y, maxX = first.x, maxY = first.y
        for point in points.dropFirst() {
            minX = min(minX, point.x); minY = min(minY, point.y)
            maxX = max(maxX, point.x); maxY = max(maxY, point.y)
        }
        return PdfRect(x: minX, y: minY, width: maxX - minX, height: maxY - minY)
    }
}

/// One annotation edit — the annotation subset of `FfiEditCommand`.
enum AnnotationEdit: Equatable {
    /// A rect-based mark: highlight, underline, strikeout or shape.
    case add(kind: AnnotationKind, page: Int, rect: PdfRect, color: PdfColor)
    /// A freehand stroke through `points`, in page space.
    case addInk(page: Int, points: [PdfPoint], color: PdfColor)
    /// A sticky note whose icon sits at `rect`.
    case addNote(page: Int, rect: PdfRect, contents: String)
    case remove(id: UInt64)
    /// Shifts by `dx`/`dy` page-space points.
    case move(id: UInt64, dx: Double, dy: Double)
    /// Replaces a rect-based annotation's rect.
    case resize(id: UInt64, rect: PdfRect)
    case restyle(id: UInt64, color: PdfColor)
    /// A visible text box; the core wraps and validates the text.
    case addFreeText(page: Int, rect: PdfRect, contents: String)
    /// Replaces a note's or a text box's text.
    case setContents(id: UInt64, contents: String)
}

/// A text box's text as the core lays it out — mirrors `FfiFreeTextLayout`.
/// A shell draws these lines; it never wraps the text itself.
struct FreeTextLayout: Equatable {
    struct Line: Equatable {
        let text: String
        /// From the box's left edge, in points.
        let xPt: Double
        /// From the box's top edge down to the line's baseline, in points.
        let baselineFromTopPt: Double
    }

    let fontSizePt: Double
    let lines: [Line]
    /// The text did not fit the box.
    let overflow: Bool
}

/// How a page is turned as drawn — mirrors `FfiPageRotation`.
enum PageRotation: Equatable {
    case none, clockwise90, clockwise180, clockwise270
}

/// One page as drawn: its size with the turn applied, the turn, and the
/// display units per point — mirrors `FfiPagePlacement`.
struct PagePlacement: Equatable {
    let width: Double
    let height: Double
    let rotation: PageRotation
    let scale: Double
}

/// A page-space rect placed on the drawn page, in display units with a
/// top-left origin — mirrors `FfiPlacedRect`.
struct PlacedRect: Equatable {
    let left: Double
    let top: Double
    let width: Double
    let height: Double
}

/// A page-space point placed on the drawn page — mirrors the `FfiPoint` that
/// `place_point` returns, in display units with a top-left origin.
struct PlacedPoint: Equatable {
    let left: Double
    let top: Double
}

/// Default, harmless annotation and placement answers, so conformers that
/// predate annotations — test doubles among them — need not grow them.
extension PdfCoreClient {
    func annotations(document: any PdfDocument) -> [Annotation] { [] }
    func annotationEditingAllowed(document: any PdfDocument) -> Bool { false }
    func comments(document: any PdfDocument) -> [Comment] { [] }

    func applyAnnotationEdit(document: any PdfDocument, edit: AnnotationEdit) throws {
        throw EditFailure.failed("this client does not support annotations")
    }

    func freeTextLayout(contents: String, width: Double, height: Double) throws -> FreeTextLayout {
        FreeTextLayout(fontSizePt: 12, lines: [], overflow: false)
    }

    func insertImageStamp(document: any PdfDocument, page: Int, imageBytes: Data, rect: PdfRect) throws {
        throw EditFailure.failed("this client does not support stamps")
    }

    func stampPlacement(imageBytes: Data, anchor: PdfPoint) throws -> PdfRect {
        throw EditFailure.failed("this client does not support stamps")
    }

    /// Unturned placement, for clients without the core's math.
    func placeRect(_ rect: PdfRect, on page: PagePlacement) -> PlacedRect {
        PlacedRect(
            left: rect.x * page.scale,
            top: (page.height - rect.y - rect.height) * page.scale,
            width: rect.width * page.scale,
            height: rect.height * page.scale
        )
    }

    func placePoint(_ point: PdfPoint, on page: PagePlacement) -> PlacedPoint {
        PlacedPoint(left: point.x * page.scale, top: (page.height - point.y) * page.scale)
    }

    func pointToPdf(x: Double, y: Double, on page: PagePlacement) -> PdfPoint {
        PdfPoint(x: x / page.scale, y: page.height - y / page.scale)
    }
}

/// A comment the file carries, or a note this session added — mirrors
/// `FfiComment`. Reading comments never adds the file's annotations to the
/// editable set, and works without edit permission.
struct Comment: Equatable {
    let page: Int
    let rect: PdfRect
    let contents: String
    let author: String?
    let date: String?
    /// Only a pending note has a session annotation id to select.
    let annotationId: UInt64?

    /// "Author · Date", leaving out what the file does not say.
    var byline: String {
        [author, date].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · ")
    }
}
