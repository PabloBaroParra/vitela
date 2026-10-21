// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Add watermark: the same text stamped across the pages the reader picks —
/// Linux's `content_edit/watermark`, Windows' `AddWatermarkAsync`.
///
/// A dialog rather than an armed click like Add Text, for the reason both
/// give: the core centres the stamp on every page it names, so there is no
/// position to pick, and what it does need — a page list — no click can
/// give. It is page content, so it answers to the content-editing
/// permission and runs through the page-work pipeline like every other
/// content edit: recorded, the preview re-derived, one undo step.
extension ViewerStore {
    /// The same three choices Export images offers, read by the same
    /// grammar — Linux shares `resolve_pages`' contract between the two.
    typealias WatermarkPages = ImageExportPages

    /// The stamp the dialog's answers make, started as page work; or why
    /// the dialog must stay open. Everything the reader can correct in
    /// place is checked here, before the core is asked; what only the
    /// writer knows (a character Helvetica cannot show) comes back as the
    /// work's failure.
    func beginWatermark(
        pages: WatermarkPages, text: String, fontSize: Double, opacityPercent: Double, rotationDegrees: Double
    ) -> (problem: String?, request: PageWorkRequest?) {
        guard document != nil, !pageWorkInFlight else { return (nil, nil) }
        guard editingAllowed else { return (ContentEditing.forbidden, nil) }
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return (Watermarking.blankText, nil) }
        let total = pageSlots.count
        guard total > 0 else { return (Watermarking.noPages, nil) }
        let chosen: [Int]
        switch pages {
        case .all:
            chosen = Array(0..<total)
        case .current:
            // Clamped for the reason export gives: the readout can sit one
            // past the end right after a deletion.
            chosen = [min(currentPage, total - 1)]
        case let .range(typed):
            guard !typed.trimmingCharacters(in: .whitespaces).isEmpty else { return (Watermarking.blankRange, nil) }
            do {
                chosen = try client.parsePageSelection(typed, totalPages: total)
            } catch {
                return (Self.editFailure(error).localizedDescription, nil)
            }
        }
        if let problem = Self.watermarkLookProblem(fontSize: fontSize, opacityPercent: opacityPercent, rotationDegrees: rotationDegrees) {
            return (problem, nil)
        }
        // Kept as typed, leading spaces included: the stamp is centred on
        // its own width and the reader may have meant them — Linux's rule.
        let watermark = Watermark(
            pages: chosen, text: text, fontSize: fontSize, opacity: opacityPercent / 100, rotationDegrees: rotationDegrees
        )
        return (nil, beginContentEdit(.addWatermark(watermark)))
    }

    /// A value outside the dialog's ranges — only reachable by typing, the
    /// steppers stop at the ends.
    private static func watermarkLookProblem(fontSize: Double, opacityPercent: Double, rotationDegrees: Double) -> String? {
        func text(_ range: ClosedRange<Double>) -> String { "\(Int(range.lowerBound)) and \(Int(range.upperBound))" }
        if !Watermarking.fontSizes.contains(fontSize) { return "Size must be between \(text(Watermarking.fontSizes)) pt." }
        if !Watermarking.opacityPercents.contains(opacityPercent) {
            return "Opacity must be between \(text(Watermarking.opacityPercents)) %."
        }
        if !Watermarking.rotations.contains(rotationDegrees) { return "Angle must be between \(text(Watermarking.rotations))°." }
        return nil
    }
}
