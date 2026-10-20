// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// What a watermark paints — mirrors `pdf_document::WatermarkMark`.
enum WatermarkMark: Equatable {
    /// Helvetica, `fontSize` points.
    case text(String, fontSize: Double)
    /// The picked PNG or JPEG, untouched — decoding is the core's job.
    /// `scale` is the share of the page it may cover, `0` exclusive to `1`
    /// inclusive, fitted with its aspect kept.
    case picture(Data, scale: Double)
}

/// Whether a stamp is painted over the page content or behind it — mirrors
/// `FfiWatermarkLayer`. Behind does not show on a scanned page, whose
/// content is one opaque picture.
enum WatermarkLayer: Equatable {
    case overContent, underContent
}

/// Which kind of mark the dialog is building.
enum WatermarkKind: Equatable {
    case text, picture
}

/// One "Add watermark" stamp — the same mark centred on every page it names,
/// as one undo step. Mirrors `FfiWatermark` / `FfiImageWatermark` minus a
/// text stamp's colour, which is the core's: no shell's dialog asks for one.
struct Watermark: Equatable {
    /// Zero-based page positions, ascending.
    let pages: [Int]
    let mark: WatermarkMark
    /// `0` exclusive to `1` inclusive — already converted from percent.
    let opacity: Double
    /// Counter-clockwise, as the reader sees the page.
    let rotationDegrees: Double
    let layer: WatermarkLayer
}

/// The look the dialog starts from for one kind — the core's
/// `default_watermark` / `default_image_watermark`, in the dialog's units —
/// so the first stamp matches the other shells'. Switching kind starts from
/// that kind's look again: diagonal text over the content, an upright
/// picture behind it (Linux's `look_defaults`).
struct WatermarkLook: Equatable {
    /// Points for text; percent of the page for a picture.
    let size: Double
    let opacityPercent: Double
    let rotationDegrees: Double
    let layer: WatermarkLayer
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
    /// A picture's share of the page, in percent: the same "nobody would
    /// find it" floor; 100 is the core's ceiling, touching two edges.
    static let scalePercents = 5.0...100.0
    /// What the text field starts with, as on Linux and Windows.
    static let defaultText = "DRAFT"

    static let blankText = "Type the text to stamp on the pages."
    static let noPicture = "Choose a PNG or JPEG picture to stamp on the pages."
    /// Linux's: the core's own message names a decoder error.
    static let notAPicture = "That file could not be read as a PNG or JPEG picture. Choose another one."
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
    func defaultWatermarkLook(_ kind: WatermarkKind) -> WatermarkLook {
        switch kind {
        case .text: return WatermarkLook(size: 60, opacityPercent: 30, rotationDegrees: 45, layer: .overContent)
        case .picture: return WatermarkLook(size: 50, opacityPercent: 30, rotationDegrees: 0, layer: .underContent)
        }
    }
}
