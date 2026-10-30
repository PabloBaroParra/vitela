// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// What a copy or cut holds (B25 T-209): a one-page PDF of the items,
/// repainted with their own resources, and the box they came from —
/// mirrors `FfiContentClip`. Self-contained, so it pastes into another
/// document as well as this one.
struct ContentClip: Equatable {
    let pdf: Data
    let bbox: PdfRect
}

extension ContentEditing {
    static let copyRefused = "This content cannot be copied. Save first if it has an unsaved edit; some documents do not permit copying."
    static let nothingToCopy = "Select something to copy first."

    static func copied(_ count: Int, cut: Bool) -> String {
        let what = cut ? "Cut" : "Copied"
        return "\(what) \(count) \(count == 1 ? "item" : "items"). Point where it should go and press ⌘V."
    }
}

/// The content clipboard in Edit Content — Windows'
/// `MainWindow.ContentClipboard.cs`. The clip lives with the shell, not on
/// the system clipboard: only this app reads it.
extension ViewerStore {
    /// What a copy or cut acts on: the multi-selection, or the one image or
    /// shape selected on its own.
    var clipboardSelection: ContentSelection? {
        guard contentEditMode else { return nil }
        if let selection = contentEdit.multiSelection, !selection.isEmpty { return selection }
        if let image = contentEdit.selectedImage { return ContentSelection(page: image.page).toggled(image) }
        if let path = contentEdit.selectedPath { return ContentSelection(page: path.page).toggled(path) }
        return nil
    }

    /// Copies the clipboard selection, or says why not. Main thread, like
    /// the other questions the shell asks the core between edits.
    func copyContentSelection() -> (clip: ContentClip, selection: ContentSelection)? {
        guard let document, !pageWorkInFlight else { return nil }
        guard let selection = clipboardSelection else {
            editStatus = ContentEditing.nothingToCopy
            return nil
        }
        do {
            let clip = try client.copyContent(document: document, selection: selection)
            editStatus = ContentEditing.copied(selection.count, cut: false)
            return (clip, selection)
        } catch {
            editStatus = Self.editFailure(error).localizedDescription
            return nil
        }
    }

    /// Cut: the copy, then the selection's removal as one undo step — what
    /// the core's `cut_content_selection` does, through the page work.
    func beginContentCut() -> (clip: ContentClip, request: PageWorkRequest)? {
        guard let (clip, selection) = copyContentSelection(),
              let request = beginContentEdit(.removeSelection(selection))
        else { return nil }
        contentEdit.multiSelection = nil
        contentEdit.selectedImage = nil
        contentEdit.selectedPath = nil
        return (clip, request)
    }

    /// `clip` on `page` with its top-left corner at `point`.
    func beginContentPaste(_ clip: ContentClip, page: Int, at point: PdfPoint) -> PageWorkRequest? {
        guard contentEditMode, pageSlots.indices.contains(page) else { return nil }
        return beginContentEdit(.pasteContent(page: page, clip: clip, left: point.x, top: point.y))
    }
}
