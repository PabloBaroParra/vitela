import Foundation

/// Organize's Documents view on macOS: a card's actions and dragging a card
/// to one of the gaps between them. The rules are the store's
/// (`ViewerStore+Blocks.swift`); a drag carries a one-time token, as the
/// Pages view's does, so text dropped from another app cannot pass for one.
extension ViewerViewModel {
    /// What the open file's own block is called.
    var baseBlockName: String { documentName ?? "Document" }

    func showOrganizeDocuments(_ on: Bool) {
        organizeShowsDocuments = on
        organizeBlockDrag = nil
        organizePageDrag = nil
        store.editStatus = on ? DocumentBlocks.documentsHint : DocumentBlocks.pagesHint
    }

    func editBlock(_ position: Int, _ action: ViewerStore.BlockAction) {
        enqueuePageWork(store.beginBlockEdit(position, action))
    }

    func beginBlockDrag(position: Int) -> String {
        let drag = OrganizePageDrag(page: position, generation: store.generation)
        organizeBlockDrag = drag
        return drag.token
    }

    /// The gap at `slot` received a drop carrying `token`. Returns whether a
    /// move was issued.
    @discardableResult
    func dropBlock(token: String, atSlot slot: Int) -> Bool {
        guard let drag = organizeBlockDrag, drag.token == token else { return false }
        organizeBlockDrag = nil
        guard drag.generation == store.generation, let request = store.beginBlockEdit(drag.page, .move(toSlot: slot)) else { return false }
        enqueuePageWork(request)
        return true
    }
}
