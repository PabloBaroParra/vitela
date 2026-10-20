// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// One "Add watermark" stamp — the same text centred on every page it
/// names, as one undo step. Mirrors `FfiWatermark` minus its colour, which is
/// the core's: no shell's dialog asks for one.
struct Watermark: Equatable {
    /// Zero-based page positions, ascending.
    let pages: [Int]
    let text: String
    /// In points.
    let fontSize: Double
    /// `0` exclusive to `1` inclusive — already converted from percent.
    let opacity: Double
    /// Counter-clockwise, as the reader sees the page.
    let rotationDegrees: Double
}

/// The look the dialog starts from — the core's `default_watermark`, in the
/// dialog's units — so the first stamp matches Linux's, Windows' and
/// Android's.
struct WatermarkLook: Equatable {
    let fontSize: Double
    let opacityPercent: Double
    let rotationDegrees: Double
}

/// The dialog's ranges and wording — Linux's `content_edit::watermark::options`
/// and Windows' `WatermarkLimits`, so every dialog refuses the same values.
enum Watermarking {
    /// The floor keeps the stamp legible; the ceiling is past what fits
    /// across portrait A4 at the default angle.
    static let fontSizes = 8.0...200.0
    /// 5, not 1: the core refuses only a fully transparent stamp, but one at
    /// 1% is a watermark nobody will ever find.
    static let opacityPercents = 5.0...100.0
    static let rotations = -180.0...180.0
    /// What the text field starts with, as on Linux and Windows.
    static let defaultText = "DRAFT"

    static let blankText = "Type the text to stamp on the pages."
    static let blankRange = "Type which pages to stamp, for example 1-3,7."
    static let noPages = "This document has no pages to stamp."
    static let cancelled = "Watermark cancelled."
    static let hint = "A watermark is painted into the pages themselves and is one undo step."

    /// What the status line says once the stamp is recorded.
    static func summary(count: Int) -> String {
        "Watermark added to \(count) \(count == 1 ? "page" : "pages")."
    }

    /// Linux's refusal for a character Helvetica cannot show, in the user's
    /// words rather than the core's font-resource ones.
    static func encodingGap(_ character: String) -> String {
        "The watermark font cannot show “\(character)”. Use Latin letters, digits and punctuation."
    }
}

/// Defaults for clients without the core's watermark — kept beside the
/// watermark types rather than in `PdfCore.swift`'s general defaults.
extension PdfCoreClient {
    func defaultWatermarkLook() -> WatermarkLook {
        WatermarkLook(fontSize: 60, opacityPercent: 30, rotationDegrees: 45)
    }
}
