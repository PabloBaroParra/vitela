// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// How a run's font is stored — mirrors `FfiFontKind`.
enum ContentFontKind: Equatable {
    case standard14, embeddedSimple, embeddedComposite
}

/// One line of text the page itself paints, as the core parsed it — mirrors
/// `FfiContentTextRun`. Sent back unchanged with an edit, so the core can
/// find the run again and amend a pending retype of it.
struct ContentTextRun: Equatable, Identifiable {
    let id: UInt64
    let page: Int
    let bbox: PdfRect
    let resourceFontName: String
    let fontKind: ContentFontKind
    let text: String

    /// The core cannot re-encode a composite (CID) font, so retyping such a
    /// run swaps in a standard font — Windows' `RequiresFontSubstitution`.
    var substitutesFont: Bool { fontKind == .embeddedComposite }
}

/// One image the page paints — mirrors `FfiContentImageItem`.
struct ContentImage: Equatable {
    let id: UInt64
    let page: Int
    let bbox: PdfRect
    let resourceXObjectName: String?
}

/// One page's editable content — mirrors `FfiPageContent`.
struct PageContent: Equatable {
    let textRuns: [ContentTextRun]
    let images: [ContentImage]
}

/// A change to what a page paints — the content half of `FfiEditCommand`.
enum ContentEdit: Equatable {
    /// Retypes `run`; a composite-font run gets a standard font instead.
    case retypeText(ContentTextRun, to: String)
    case removeText(ContentTextRun)
    /// Moves `run` so its box lands at `to`, keeping its text, font and size.
    case moveText(ContentTextRun, to: PdfRect)
    /// A new line of Helvetica: `rect`'s height is its size, its top-left
    /// corner where the line starts.
    case insertText(page: Int, rect: PdfRect, text: String)

    /// What the status line says once the core took it — the Windows and
    /// Android wording.
    var done: String {
        switch self {
        case .retypeText: return "Text updated."
        case .removeText: return "Text deleted."
        case .moveText: return "Text moved."
        case .insertText: return "Text inserted."
        }
    }
}

/// Content-edit hit-testing and wording, shared with the Android shell's
/// `ContentEdit.kt` and Windows' `ContentHitTest`.
enum ContentEditing {
    static let armed = "Click text to retype or delete it, or drag it to move it."
    static let off = "Content editing off."
    static let missed = "Nothing to edit there. Click text to retype or delete it, or drag it to move it."
    static let positionUnchanged = "Text position unchanged."
    /// Windows' refusal to retype a run whose move is not saved yet.
    static let movedNotSaved = "This text was moved and not yet saved — save and reopen before retyping it."
    static let insertPrompt = "Click the page where the new text's top-left corner should go."
    static let insertCancelled = "Insert cancelled."
    static let insertEmpty = "Enter a nonempty single line of text."
    static let insertSizeInvalid = "Text size must be between 1 and 72 pt."
    /// The size a new line starts at, in points — Windows' and Android's.
    static let defaultInsertedTextSize = 14.0

    /// A line the core can insert: not blank, and one line.
    static func insertableText(_ text: String) -> Bool {
        !text.trimmingCharacters(in: .whitespaces).isEmpty && !text.contains(where: { $0 == "\n" || $0 == "\r" })
    }

    /// The box a new line at `size` points takes with its top-left corner at
    /// `at` — Android's `insertedTextRect`; `nil` outside 1...72 pt.
    static func insertedTextRect(at: PdfPoint, size: Double) -> PdfRect? {
        guard size.isFinite, (1...72).contains(size) else { return nil }
        return PdfRect(x: at.x, y: at.y - size, width: size, height: size)
    }

    /// How far, in screen points, a press must travel to be a drag rather
    /// than a click — Linux's `CLICK_EPSILON_PX`.
    static let clickEpsilon = 4.0
    static let forbidden = "This document does not permit content changes."
    static let fontSubstituted = "This text's font cannot be kept. What you type will use a standard font."
    static let emptyRetype = "Type the new text, or delete the line instead."
    /// How far off a run, in screen points, a click may land and still mean it.
    static let reach = 6.0

    /// The run a click at `point` meant: one it lands inside, else the
    /// nearest within `reach` page points. Among runs equally near — a run
    /// drawn over a wider one — the smallest wins: it is the one whose edges
    /// the reader can see.
    static func textRun(at point: PdfPoint, in runs: [ContentTextRun], reach: Double) -> ContentTextRun? {
        var best: (run: ContentTextRun, distance: Double, area: Double)?
        for run in runs {
            let distance = distance(from: point, to: run.bbox)
            guard distance <= reach else { continue }
            let area = run.bbox.width * run.bbox.height
            if let current = best, current.distance < distance || (current.distance == distance && current.area <= area) { continue }
            best = (run, distance, area)
        }
        return best?.run
    }

    static func distance(from point: PdfPoint, to rect: PdfRect) -> Double {
        let dx = max(rect.x - point.x, point.x - (rect.x + rect.width), 0)
        let dy = max(rect.y - point.y, point.y - (rect.y + rect.height), 0)
        return (dx * dx + dy * dy).squareRoot()
    }
}
