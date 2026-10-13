// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Hover text for sticky notes: resting the pointer on a note shows what it
/// says, because the page draws a note only as an icon — Linux's
/// `annotations/tooltip.rs`, Windows' `MainWindow.NoteHover.cs`.
struct NoteTooltip: Equatable {
    /// Where the note sits on the drawn page, in display points.
    let frame: PlacedRect
    let text: String
}

extension ViewerStore {
    /// Windows' `NoteTooltipMaxWidth`.
    static let noteTooltipMaxLength = 600

    /// The text of the topmost note under `point` on `page`, or `nil` —
    /// Linux's rule: the topmost annotation wins, as for a click, and when it
    /// is not a note, or is one with nothing in it, nothing shows rather than
    /// reaching through it to a note underneath.
    func noteText(page: Int, at point: PdfPoint) -> String? {
        guard let top = annotations(onPage: page).last(where: { $0.bounds?.contains(point) == true }),
              top.kind == .textNote
        else { return nil }
        return Self.tooltipText(top.contents)
    }

    /// Every note on `page` with something to say, placed at `zoom`, in paint
    /// order — for a platform that hangs a tooltip on each note's area.
    func noteTooltips(page: Int, zoom: Double) -> [NoteTooltip] {
        guard let placement = placement(ofPage: page, zoom: zoom) else { return [] }
        return annotations(onPage: page).compactMap { note in
            guard note.kind == .textNote, let bounds = note.bounds, let text = Self.tooltipText(note.contents) else { return nil }
            return NoteTooltip(frame: client.placeRect(bounds, on: placement), text: text)
        }
    }

    /// A note's text trimmed for a tooltip; `nil` when there is none.
    static func tooltipText(_ contents: String?) -> String? {
        guard let text = contents?.trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty else { return nil }
        return text.count > noteTooltipMaxLength ? String(text.prefix(noteTooltipMaxLength)) + "…" : text
    }
}
