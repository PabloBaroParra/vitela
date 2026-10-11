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
