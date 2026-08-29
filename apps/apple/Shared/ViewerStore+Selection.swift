// Drag-to-select text on a page: fetching a page's characters (cached, once
// per page), caret hit-testing, and the range a shell paints/copies. Mirrors
// the GTK shell's `app/selection.rs` and Windows' `MainWindow.Selection.cs`.
import Foundation

extension ViewerStore {
    /// A self-contained unit of page-characters work, mirroring `RenderRequest`.
    struct PageCharactersRequest {
        let generation: UInt
        let document: any PdfDocument
        let page: Int
    }

    /// `nil` when there is nothing to fetch — no document open, or `page`'s
    /// characters are already cached.
    func beginPageCharactersFetch(page: Int) -> PageCharactersRequest? {
        guard pageCharactersCache[page] == nil, let document else { return nil }
        return PageCharactersRequest(generation: generation, document: document, page: page)
    }

    /// Thread-safe: touches only the immutable `client` and the request's
    /// own captured document. Callers run this off the main thread.
    func pageCharactersResult(for request: PageCharactersRequest) -> (any PageCharacters)? {
        try? client.pageCharacters(document: request.document, page: request.page)
    }

    func applyPageCharacters(_ characters: (any PageCharacters)?, for request: PageCharactersRequest) {
        guard request.generation == generation, let characters else { return }
        pageCharactersCache[request.page] = characters
    }

    /// Starts (or restarts, on a fresh click) a selection at the caret
    /// nearest `(xPt, yPt)` on `page`. A no-op if that page's characters
    /// haven't been fetched yet, or the page has no positioned text.
    func beginSelection(page: Int, xPt: Double, yPt: Double) {
        guard let caret = pageCharactersCache[page]?.caretAt(xPt: xPt, yPt: yPt) else { return }
        selection = TextSelection(page: page, anchor: caret, focus: caret)
    }

    /// Moves the selection's `focus` to the caret nearest `(xPt, yPt)` on
    /// `page`, keeping the original `anchor`. A no-op if the drag has left
    /// the page the selection started on, or that page has no caret there.
    func extendSelection(page: Int, xPt: Double, yPt: Double) {
        guard var current = selection, current.page == page,
              let caret = pageCharactersCache[page]?.caretAt(xPt: xPt, yPt: yPt)
        else { return }
        current.focus = caret
        selection = current
    }

    func clearSelection() {
        selection = nil
    }

    /// The selected text, or `nil` when there is no selection (or it is
    /// empty — a plain click with no drag).
    var selectedText: String? {
        guard let selection, let characters = pageCharactersCache[selection.page] else { return nil }
        let text = characters.textIn(anchor: selection.anchor, focus: selection.focus)
        return text.isEmpty ? nil : text
    }

    /// The rects a view paints for the current selection on `page`.
    func selectionRects(forPage page: Int) -> [TextRect] {
        guard let selection, selection.page == page, let characters = pageCharactersCache[page] else { return [] }
        return characters.rectsIn(anchor: selection.anchor, focus: selection.focus)
    }
}
