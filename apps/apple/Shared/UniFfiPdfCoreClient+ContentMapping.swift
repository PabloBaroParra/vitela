// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Content items between the generated UniFFI records and the shared
/// types, for every file that sends them to the core — single edits
/// (`+Content.swift`) and whole groups (`+ContentGroups.swift`).
extension ContentTextRun {
    init(_ run: FfiContentTextRun) {
        self.init(
            id: run.id, page: Int(run.page), bbox: PdfRect(run.bbox), resourceFontName: run.resourceFontName,
            fontKind: ContentFontKind(run.fontKind), text: run.text
        )
    }
}

extension FfiContentTextRun {
    init(_ run: ContentTextRun) {
        self.init(
            id: run.id, page: UInt32(run.page), bbox: FfiRect(run.bbox), resourceFontName: run.resourceFontName,
            fontKind: FfiFontKind(run.fontKind), text: run.text
        )
    }
}

extension ContentImage {
    init(_ image: FfiContentImageItem) {
        self.init(id: image.id, page: Int(image.page), bbox: PdfRect(image.bbox), resourceXObjectName: image.resourceXobjectName)
    }
}

extension FfiContentImageItem {
    init(_ image: ContentImage) {
        self.init(id: image.id, page: UInt32(image.page), bbox: FfiRect(image.bbox), resourceXobjectName: image.resourceXObjectName)
    }
}

extension ContentTextBlock {
    init(_ block: FfiTextBlock) {
        self.init(runs: block.runs.map(ContentTextRun.init), bbox: PdfRect(block.bbox), text: block.text)
    }
}

extension ContentPath {
    init(_ path: FfiContentPathItem) {
        self.init(id: path.id, page: Int(path.page), bbox: PdfRect(path.bbox), paint: PathPaint(path.paint))
    }
}

extension FfiContentPathItem {
    init(_ path: ContentPath) {
        self.init(id: path.id, page: UInt32(path.page), bbox: FfiRect(path.bbox), paint: FfiPathPaint(path.paint))
    }
}

extension PathPaint {
    init(_ paint: FfiPathPaint) {
        switch paint {
        case .stroke: self = .stroke
        case .fill: self = .fill
        case .fillAndStroke: self = .fillAndStroke
        }
    }
}

extension FfiPathPaint {
    init(_ paint: PathPaint) {
        switch paint {
        case .stroke: self = .stroke
        case .fill: self = .fill
        case .fillAndStroke: self = .fillAndStroke
        }
    }
}

extension ContentFontKind {
    init(_ kind: FfiFontKind) {
        switch kind {
        case .standard14: self = .standard14
        case .embeddedSimple: self = .embeddedSimple
        case .embeddedComposite: self = .embeddedComposite
        }
    }
}

extension FfiFontKind {
    init(_ kind: ContentFontKind) {
        switch kind {
        case .standard14: self = .standard14
        case .embeddedSimple: self = .embeddedSimple
        case .embeddedComposite: self = .embeddedComposite
        }
    }
}

private extension PdfRect {
    init(_ rect: FfiRect) {
        self.init(x: rect.x, y: rect.y, width: rect.width, height: rect.height)
    }
}

private extension FfiRect {
    init(_ rect: PdfRect) {
        self.init(x: rect.x, y: rect.y, width: rect.width, height: rect.height)
    }
}
