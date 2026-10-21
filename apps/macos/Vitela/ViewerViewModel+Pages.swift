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
    /// `landed`, when given, hears the outcome on the main thread once the
    /// store has applied it — a sheet that waits to know whether to close.
    func enqueuePageWork(_ request: ViewerStore.PageWorkRequest?, landed: ((ViewerStore.PageWorkOutcome) -> Void)? = nil) {
        guard let request else { return }
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let outcome = self.store.pageWorkResult(for: request)
            DispatchQueue.main.async {
                self.store.applyPageWork(outcome, for: request)
                landed?(outcome)
            }
        }
    }

    // MARK: - Dragging a page to a new position

    /// Starts dragging `page`'s card. The payload is a one-time token, not
    /// the page number — Windows' `OrganizePageDrag.Token` — so text dropped
    /// from another app, or a drag begun before the layout changed, can never
    /// pass for a page move.
    func beginPageDrag(page: Int) -> String {
        let drag = OrganizePageDrag(page: page, generation: store.generation)
        organizePageDrag = drag
        return drag.token
    }

    /// The card at `target` received a drop carrying `token`. Moves the
    /// dragged page so it ends up at `target`, the position the core's
    /// `MovePages` gives `to`. Returns whether a move was issued.
    @discardableResult
    func dropPage(token: String, on target: Int) -> Bool {
        guard let drag = organizePageDrag, drag.token == token else { return false }
        organizePageDrag = nil
        guard drag.generation == store.generation, drag.page != target else { return false }
        editPages(.move(from: drag.page, to: target))
        return true
    }
}

/// A page card being dragged, and the layout it was picked up from.
struct OrganizePageDrag {
    let page: Int
    let generation: UInt
    let token = UUID().uuidString
}
