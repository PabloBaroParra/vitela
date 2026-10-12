// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Annotations and page placement through the generated UniFFI API — the
/// calls Windows' facade makes (`GeneratedPdfCore.cs`).
extension UniFfiPdfCoreClient {
    func annotations(document: any PdfDocument) -> [Annotation] {
        guard let handle = try? handle(of: document) else { return [] }
        return handle.annotations().map(Annotation.init)
    }

    func annotationEditingAllowed(document: any PdfDocument) -> Bool {
        (try? handle(of: document).annotationEditingAllowed()) ?? false
    }

    func applyAnnotationEdit(document: any PdfDocument, edit: AnnotationEdit) throws {
        let handle = try handle(of: document)
        try Self.mapEditErrors { try applyEdit(handle: handle, command: FfiEditCommand(edit)) }
    }

    func freeTextLayout(contents: String, width: Double, height: Double) throws -> FreeTextLayout {
        try Self.mapEditErrors { FreeTextLayout(try freetextLayout(contents: contents, widthPt: width, heightPt: height)) }
    }

    func insertImageStamp(document: any PdfDocument, page: Int, imageBytes: Data, rect: PdfRect) throws {
        let handle = try handle(of: document)
        try Self.mapEditErrors {
            try Vitela.insertImageStamp(handle: handle, pageIndex: UInt32(page), imageBytes: imageBytes, rect: FfiRect(rect))
        }
    }

    func stampPlacement(imageBytes: Data, anchor: PdfPoint) throws -> PdfRect {
        try Self.mapEditErrors { PdfRect(try Vitela.stampPlacement(imageBytes: imageBytes, anchorX: anchor.x, anchorY: anchor.y)) }
    }

    func placeRect(_ rect: PdfRect, on page: PagePlacement) -> PlacedRect {
        let placed = Vitela.placeRect(rect: FfiRect(rect), page: FfiPagePlacement(page))
        return PlacedRect(left: placed.left, top: placed.top, width: placed.width, height: placed.height)
    }

    func placePoint(_ point: PdfPoint, on page: PagePlacement) -> PlacedPoint {
        let placed = Vitela.placePoint(point: FfiPoint(x: point.x, y: point.y), page: FfiPagePlacement(page))
        return PlacedPoint(left: placed.x, top: placed.y)
    }

    func pointToPdf(x: Double, y: Double, on page: PagePlacement) -> PdfPoint {
        let point = Vitela.pointToPdf(point: FfiPoint(x: x, y: y), page: FfiPagePlacement(page))
        return PdfPoint(x: point.x, y: point.y)
    }
}

extension PageDimensions {
    init(_ page: FfiPageDimensions) {
        self.init(width: page.widthPt, height: page.heightPt, rotation: PageRotation(page.rotation))
    }
}

private extension PageRotation {
    init(_ rotation: FfiPageRotation) {
        switch rotation {
        case .none: self = .none
        case .clockwise90: self = .clockwise90
        case .clockwise180: self = .clockwise180
        case .clockwise270: self = .clockwise270
        }
    }
}

private extension FfiPageRotation {
    init(_ rotation: PageRotation) {
        switch rotation {
        case .none: self = .none
        case .clockwise90: self = .clockwise90
        case .clockwise180: self = .clockwise180
        case .clockwise270: self = .clockwise270
        }
    }
}

private extension FfiPagePlacement {
    init(_ page: PagePlacement) {
        self.init(widthPt: page.width, heightPt: page.height, rotation: FfiPageRotation(page.rotation), scale: page.scale)
    }
}

private extension FfiRect {
    init(_ rect: PdfRect) {
        self.init(x: rect.x, y: rect.y, width: rect.width, height: rect.height)
    }
}

private extension PdfRect {
    init(_ rect: FfiRect) {
        self.init(x: rect.x, y: rect.y, width: rect.width, height: rect.height)
    }
}

private extension FfiColor {
    init(_ color: PdfColor) {
        self.init(r: color.r, g: color.g, b: color.b)
    }
}

private extension PdfColor {
    init(_ color: FfiColor) {
        self.init(r: color.r, g: color.g, b: color.b)
    }
}

private extension Annotation {
    init(_ annotation: FfiAnnotation) {
        var rect: PdfRect?
        var points: [PdfPoint] = []
        var color: PdfColor?
        var contents: String?
        var layout: FreeTextLayout?
        let kind: AnnotationKind
        switch annotation.kind {
        case let .highlight(r, c): kind = .highlight; rect = PdfRect(r); color = PdfColor(c)
        case let .underline(r, c): kind = .underline; rect = PdfRect(r); color = PdfColor(c)
        case let .strikeout(r, c): kind = .strikeout; rect = PdfRect(r); color = PdfColor(c)
        case let .shape(r, c): kind = .shape; rect = PdfRect(r); color = PdfColor(c)
        case let .ink(p, c): kind = .ink; points = p.map { PdfPoint(x: $0.x, y: $0.y) }; color = PdfColor(c)
        case let .textNote(r, text): kind = .textNote; rect = PdfRect(r); contents = text
        case let .stamp(r): kind = .stamp; rect = PdfRect(r)
        case let .freeText(r, text, size, lines):
            kind = .freeText; rect = PdfRect(r); contents = text
            layout = FreeTextLayout(fontSizePt: size, lines: lines.map(FreeTextLayout.Line.init), overflow: false)
        }
        self.init(
            id: annotation.id, page: Int(annotation.page), kind: kind,
            rect: rect, points: points, color: color, contents: contents, freeTextLayout: layout
        )
    }
}

private extension FreeTextLayout {
    init(_ layout: FfiFreeTextLayout) {
        self.init(fontSizePt: layout.fontSizePt, lines: layout.lines.map(Line.init), overflow: layout.overflow)
    }
}

private extension FreeTextLayout.Line {
    init(_ line: FfiFreeTextLine) {
        self.init(text: line.text, xPt: line.xPt, baselineFromTopPt: line.baselineFromTopPt)
    }
}

private extension FfiEditCommand {
    init(_ edit: AnnotationEdit) {
        switch edit {
        case let .add(kind, page, rect, color):
            let page = UInt32(page), rect = FfiRect(rect), color = FfiColor(color)
            switch kind {
            case .underline: self = .addUnderline(page: page, rect: rect, color: color)
            case .strikeout: self = .addStrikeout(page: page, rect: rect, color: color)
            case .shape: self = .addShape(page: page, rect: rect, color: color)
            default: self = .addHighlight(page: page, rect: rect, color: color)
            }
        case let .addInk(page, points, color):
            self = .addInk(page: UInt32(page), points: points.map { FfiPoint(x: $0.x, y: $0.y) }, color: FfiColor(color))
        case let .addNote(page, rect, contents):
            self = .addTextNote(page: UInt32(page), rect: FfiRect(rect), contents: contents)
        case let .remove(id):
            self = .removeAnnotation(annotationId: id)
        case let .move(id, dx, dy):
            self = .moveAnnotation(annotationId: id, dx: dx, dy: dy)
        case let .resize(id, rect):
            self = .resizeAnnotation(annotationId: id, rect: FfiRect(rect))
        case let .restyle(id, color):
            self = .restyleAnnotation(annotationId: id, color: FfiColor(color))
        case let .addFreeText(page, rect, contents):
            self = .addFreeText(page: UInt32(page), rect: FfiRect(rect), contents: contents)
        case let .setContents(id, contents):
            self = .setAnnotationContents(annotationId: id, contents: contents)
        }
    }
}
