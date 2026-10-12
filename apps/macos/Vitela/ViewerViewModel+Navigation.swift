import Foundation

/// The Go to page prompt. Moving between pages is store state
/// (`ViewerStore+Navigation.swift`); what lives here is the macOS sheet's
/// presentation, which the toolbar button and the Go menu both open.
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
}
