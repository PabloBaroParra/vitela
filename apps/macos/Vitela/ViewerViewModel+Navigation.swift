import Foundation

/// The Go to page prompt and the Pages column. Moving between pages is store
/// state (`ViewerStore+Navigation.swift`); what lives here is the macOS
/// presentation — the sheet the toolbar button and the Go menu open, and the
/// column of pages the reader can click, Linux's and Windows' "Pages".
extension ViewerViewModel {
    func requestGoToPage() {
        guard store.pageCount > 0 else { return }
        isGoToPagePresented = true
    }

    /// Jumps to the typed 1-based page number and closes the prompt. Returns
    /// `false`, leaving the prompt open, when the text names no page.
    @discardableResult
    func goToTypedPage(_ text: String) -> Bool {
        guard let index = store.pageIndex(forTypedNumber: text) else { return false }
        isGoToPagePresented = false
        store.goToPage(index)
        return true
    }

    /// Whether a click in the Pages column can move the reader: not while
    /// Organize stands in for the pages, as Windows disables its list then.
    var canPickPageFromList: Bool { store.pageCount > 0 && !isOrganizing }

    /// A click on a page in the Pages column.
    func pickPageFromList(_ index: Int) {
        guard canPickPageFromList else { return }
        store.goToPage(index)
    }
}
