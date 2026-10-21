// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Add watermark: the same text, or the same picture, stamped across the
/// pages the reader picks, over the page content or behind it — Linux's
/// `content_edit/watermark`, Windows' `AddWatermarkAsync` (text only there).
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

    /// The mark half of the dialog's answers, as asked: a picture may not be
    /// chosen yet, and its size is in percent.
    enum WatermarkMarkAnswer: Equatable {
        case text(String, fontSize: Double)
        case picture(Data?, scalePercent: Double)
    }

    /// The stamp the dialog's answers make, started as page work; or why
    /// the dialog must stay open. Everything the reader can correct in
    /// place is checked here, before the core is asked; what only the
    /// writer knows — a character Helvetica cannot show, a file that does
    /// not decode — comes back as the work's failure.
    func beginWatermark(
        pages: WatermarkPages, mark: WatermarkMarkAnswer, opacityPercent: Double, rotationDegrees: Double, layer: WatermarkLayer
    ) -> (problem: String?, request: PageWorkRequest?) {
        guard document != nil, !pageWorkInFlight else { return (nil, nil) }
        guard editingAllowed else { return (ContentEditing.forbidden, nil) }
        let resolved: WatermarkMark
        switch Self.resolve(mark) {
        case let .success(value): resolved = value
        case let .failure(problem): return (problem.localizedDescription, nil)
        }
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
        if let problem = Self.watermarkLookProblem(opacityPercent: opacityPercent, rotationDegrees: rotationDegrees) {
            return (problem, nil)
        }
        let watermark = Watermark(
            pages: chosen, mark: resolved, opacity: opacityPercent / 100, rotationDegrees: rotationDegrees, layer: layer
        )
        return (nil, beginContentEdit(.addWatermark(watermark)))
    }

    /// The mark to stamp, or what to say instead. Text is kept as typed,
    /// leading spaces included: the stamp is centred on its own width and the
    /// reader may have meant them — Linux's rule. Only "nothing picked" is
    /// caught for a picture; whether the bytes decode is the core's answer.
    private static func resolve(_ mark: WatermarkMarkAnswer) -> Result<WatermarkMark, EditFailure> {
        switch mark {
        case let .text(text, fontSize):
            guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return .failure(.failed(Watermarking.blankText)) }
            guard Watermarking.fontSizes.contains(fontSize) else {
                return .failure(.failed("Size must be between \(Self.bounds(Watermarking.fontSizes)) pt."))
            }
            return .success(.text(text, fontSize: fontSize))
        case let .picture(bytes, scalePercent):
            guard let bytes, !bytes.isEmpty else { return .failure(.failed(Watermarking.noPicture)) }
            guard Watermarking.scalePercents.contains(scalePercent) else {
                return .failure(.failed("Size must be between \(Self.bounds(Watermarking.scalePercents)) % of the page."))
            }
            return .success(.picture(bytes, scale: scalePercent / 100))
        }
    }

    private static func bounds(_ range: ClosedRange<Double>) -> String { "\(Int(range.lowerBound)) and \(Int(range.upperBound))" }

    /// A value outside the dialog's ranges — only reachable by typing, the
    /// steppers stop at the ends.
    private static func watermarkLookProblem(opacityPercent: Double, rotationDegrees: Double) -> String? {
        if !Watermarking.opacityPercents.contains(opacityPercent) {
            return "Opacity must be between \(bounds(Watermarking.opacityPercents)) %."
        }
        if !Watermarking.rotations.contains(rotationDegrees) { return "Angle must be between \(bounds(Watermarking.rotations))°." }
        return nil
    }
}
