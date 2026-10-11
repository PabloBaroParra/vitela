import Foundation

/// The Organize view and the page edits it issues. The edits themselves are
/// store state (`ViewerStore+Pages.swift`); what lives here is the macOS
/// mode switch and getting the core work onto the serial queue.
extension ViewerViewModel {
    func editPages(_ edit: PageEdit) {
        enqueuePageWork(store.beginPageEdit(edit))
    }

    func renderThumbnail(page: Int, scale: Double) {
        guard let request = store.beginThumbnail(page: page, scale: scale) else { return }
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let result = self.store.renderResult(for: request)
            DispatchQueue.main.async { self.store.applyThumbnail(result, for: request) }
        }
    }

    /// Runs page work on the queue — behind any render of the old layout —
    /// and applies the outcome on main.
    func enqueuePageWork(_ request: ViewerStore.PageWorkRequest?) {
        guard let request else { return }
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let outcome = self.store.pageWorkResult(for: request)
            DispatchQueue.main.async { self.store.applyPageWork(outcome, for: request) }
        }
    }
}
