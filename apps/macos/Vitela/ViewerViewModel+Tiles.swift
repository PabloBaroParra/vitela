import Foundation

/// Viewport tiles on the render queue (`../apple/Shared/ViewerStore+Tiles.swift`).
extension ViewerViewModel {
    /// Called as the part of `page` in view changes; asks only for tiles
    /// that are missing, so a scroll inside the same tiles costs nothing.
    func renderTiles(page: Int, visible: PageViewRect) {
        guard let request = store.beginTiles(page: page, visible: visible) else { return }
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let result = self.store.tilesResult(for: request)
            DispatchQueue.main.async { self.store.applyTiles(result, for: request) }
        }
    }
}
