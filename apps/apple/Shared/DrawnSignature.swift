// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
//
// Geometry of a signature drawn on a pad, free of any UI type — the same
// rules as Windows' `Viewer/DrawnSignature.cs` and Android's
// `DrawnSignature.kt`: the image is cropped to the ink, so the core's stamp
// placement sizes the signature and not the empty pad around it.
import Foundation

/// A point on the signature pad, in pad points (top-left origin).
struct PadPoint: Equatable {
    let x: Double
    let y: Double
}

/// The part of the pad the image covers: the ink's bounds grown by half a
/// stroke so round caps are not clipped. `scale` shrinks it so the long side
/// fits the cap; a small signature is never enlarged.
struct SignatureFrame: Equatable {
    let left: Double
    let top: Double
    let width: Double
    let height: Double
    let scale: Double

    var pixelWidth: Int { max(1, Int((width * scale).rounded(.up))) }
    var pixelHeight: Int { max(1, Int((height * scale).rounded(.up))) }
}

enum DrawnSignature {
    /// The pad's pen, in pad points.
    static let strokeWidth = 3.0
    /// Longest side of the image: the core places a stamp at most 144 pt
    /// long, so this is already ~600 DPI.
    static let maxSidePixels = 1200

    /// A stroke needs two points to leave a line; a bare click draws nothing.
    static func hasInk(_ strokes: [[PadPoint]]) -> Bool {
        strokes.contains { $0.count >= 2 }
    }

    /// The crop around the ink, or `nil` while the pad holds no line.
    static func frame(_ strokes: [[PadPoint]], strokeWidth: Double = strokeWidth, maxSidePixels: Int = maxSidePixels) -> SignatureFrame? {
        let points = strokes.filter { $0.count >= 2 }.flatMap { $0 }
        guard let first = points.first else { return nil }
        var minX = first.x, minY = first.y, maxX = first.x, maxY = first.y
        for point in points {
            minX = min(minX, point.x); minY = min(minY, point.y)
            maxX = max(maxX, point.x); maxY = max(maxY, point.y)
        }
        let half = strokeWidth / 2
        let left = minX - half, top = minY - half
        let width = maxX + half - left, height = maxY + half - top
        let scale = min(1.0, Double(maxSidePixels) / max(max(width, height), 1.0))
        return SignatureFrame(left: left, top: top, width: width, height: height, scale: scale)
    }
}

extension ViewerStore {
    /// Arms the Stamp tool with a drawn signature: the next click on a page
    /// places it at the core's default size, keeping its proportions —
    /// Windows' `ArmDrawnSignature`.
    func armDrawnSignature(_ png: Data) {
        guard document != nil, annotationEditingAllowed else { return }
        armAnnotationTool(.stamp)
        armedStampImage = png
        editStatus = "Click a page to place your signature."
    }
}
