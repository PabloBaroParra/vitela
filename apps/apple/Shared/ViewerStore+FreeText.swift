// Text boxes (FreeText): where a click or a drag puts one, the text it asks
// for, and editing that text later. Mirrors Windows' `MainWindow.FreeText.cs`
// and `Viewer/FreeTextInput.cs` (its constants and rules) and the Linux
// `annotations/freetext.rs`. Everything about the *text* — wrapping,
// encoding, validation — stays in the core.
import Foundation

/// A text box waiting for its text: a new one at `rect`, or an existing one
/// (`editing`) whose text is being changed.
struct PendingFreeText: Equatable, Identifiable {
    var id: String { "\(page)-\(editing.map(String.init) ?? "new")-\(rect.x)-\(rect.y)" }

    let page: Int
    let rect: PdfRect
    let editing: UInt64?
    let initialText: String

    var title: String {
        "\(editing == nil ? "Add" : "Edit") text box — page \(page + 1)"
    }

    var actionTitle: String { editing == nil ? "Add" : "Save" }
}

extension ViewerStore {
    /// The box a plain click places — the shell's call; the core owns no default.
    static let freeTextDefaultSize = (width: 200.0, height: 50.0)
    /// The smallest box the core accepts for 12 pt text, rounded up so a rect
    /// never lands on the refusal boundary.
    static let freeTextMinimumSize = (width: 16.0, height: 18.0)
    /// A drag shorter than this on both axes is a click, not a drawn box.
    static let freeTextTapSlop = 4.0

    /// The Add/Save button is live only for text with something visible in it.
    static func freeTextAddEnabled(_ text: String) -> Bool {
        !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// A blank text is never sent, and the same text is not an edit; a change
    /// in whitespace alone is real — the core stores the text as typed.
    static func freeTextEditChangesText(current: String, typed: String) -> Bool {
        freeTextAddEnabled(typed) && current != typed
    }

    /// Where a placement gesture puts the box, in page space: a click hangs
    /// the default box from the click point, a drag places what it drew
    /// raised to the minimum, and either way the box stays on the page —
    /// Windows' `FreeTextInput.PlacementRect`.
    static func freeTextPlacementRect(origin: PdfPoint, release: PdfPoint, pageWidth: Double, pageHeight: Double) -> PdfRect {
        let tap = abs(release.x - origin.x) < freeTextTapSlop && abs(release.y - origin.y) < freeTextTapSlop
        var width: Double, height: Double, left: Double, top: Double
        if tap {
            (width, height) = freeTextDefaultSize
            (left, top) = (origin.x, origin.y)
        } else {
            width = max(freeTextMinimumSize.width, abs(release.x - origin.x))
            height = max(freeTextMinimumSize.height, abs(release.y - origin.y))
            (left, top) = (min(origin.x, release.x), max(origin.y, release.y))
        }
        width = min(width, pageWidth)
        height = min(height, pageHeight)
        left = min(max(left, 0), pageWidth - width)
        top = min(max(top, height), pageHeight)
        return PdfRect(x: left, y: top - height, width: width, height: height)
    }

    /// A resize that keeps the opposite corner at `anchor` and stops a side at
    /// the minimum, growing away from the anchor on the pointer's side —
    /// Windows' `FreeTextInput.ResizedRect`.
    static func freeTextResizedRect(anchor: PdfPoint, pointer: PdfPoint) -> PdfRect {
        let width = max(freeTextMinimumSize.width, abs(pointer.x - anchor.x))
        let height = max(freeTextMinimumSize.height, abs(pointer.y - anchor.y))
        return PdfRect(
            x: pointer.x >= anchor.x ? anchor.x : anchor.x - width,
            y: pointer.y >= anchor.y ? anchor.y : anchor.y - height,
            width: width,
            height: height
        )
    }

    /// A page's size in its own unturned space, where annotation rects live.
    static func unrotatedSize(_ page: PageDimensions) -> (width: Double, height: Double) {
        switch page.rotation {
        case .clockwise90, .clockwise270: return (page.height, page.width)
        case .none, .clockwise180: return (page.width, page.height)
        }
    }

    func beginFreeTextPlacement(page: Int, origin: PdfPoint, release: PdfPoint) {
        guard pageSlots.indices.contains(page) else { return }
        let size = Self.unrotatedSize(pageSlots[page].dimensions)
        let rect = Self.freeTextPlacementRect(origin: origin, release: release, pageWidth: size.width, pageHeight: size.height)
        pendingFreeText = PendingFreeText(page: page, rect: rect, editing: nil, initialText: "")
    }

    /// Opens the selected text box's text for editing.
    func beginFreeTextEdit() {
        guard annotationEditingAllowed, let box = selectedAnnotation, box.kind == .freeText, let rect = box.rect else { return }
        pendingFreeText = PendingFreeText(page: box.page, rect: rect, editing: box.id, initialText: box.contents ?? "")
    }

    /// Records the pending text box. Returns the core's refusal for the sheet
    /// to show with the typed text kept — a character the font cannot show is
    /// named — or `nil` once the sheet can close.
    func commitPendingFreeText(_ text: String) -> String? {
        guard let pending = pendingFreeText, Self.freeTextAddEnabled(text) else { return nil }
        guard annotationEditingAllowed, pageSlots.indices.contains(pending.page) else {
            pendingFreeText = nil
            return nil
        }
        if let id = pending.editing {
            guard Self.freeTextEditChangesText(current: pending.initialText, typed: text) else {
                pendingFreeText = nil
                return nil
            }
            if let refusal = applyAnnotationEdit(.setContents(id: id, contents: text), status: "Text box edited. Changes are pending save.") {
                return refusal
            }
        } else {
            let before = Set(annotations.map(\.id))
            if let refusal = applyAnnotationEdit(
                .addFreeText(page: pending.page, rect: pending.rect, contents: text),
                status: "Text box added. Changes are pending save."
            ) {
                return refusal
            }
            // A new box lands selected, handles up, as on both other shells.
            selectedAnnotationId = annotations.last { !before.contains($0.id) }?.id
        }
        pendingFreeText = nil
        return nil
    }

    func cancelPendingFreeText() {
        guard let pending = pendingFreeText else { return }
        pendingFreeText = nil
        editStatus = pending.editing == nil ? "Text box placement canceled." : "Text box edit canceled."
    }

    /// The lines to draw for `annotation` shown at `rect`: its stored layout,
    /// or — when a move or resize changes the size — the core's layout for
    /// the new box. Never a wrap computed by the shell.
    func freeTextLayout(for annotation: Annotation, at rect: PdfRect) -> FreeTextLayout? {
        if annotation.rect == rect { return annotation.freeTextLayout }
        return (try? client.freeTextLayout(contents: annotation.contents ?? "", width: rect.width, height: rect.height))
            ?? annotation.freeTextLayout
    }
}
