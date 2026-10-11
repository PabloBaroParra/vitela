// Zoom modes: an explicit factor, or one of the fit modes Linux
// (`layout.rs` `Zoom`) and Windows (`Viewer/PageZoom.cs`) open documents in.
// A fit mode is resolved per page against the viewport, so a landscape page
// in a portrait document fits on its own terms, as it does on both of them.
import Foundation

/// What the reader picked, not what it currently resolves to.
enum ZoomMode: Equatable {
    /// `ViewerStore.zoom` applies to every page.
    case custom
    /// Each page's width fills the viewport.
    case fitWidth
    /// Each whole page fits in the viewport.
    case fitPage
}

/// The area pages are laid out in, in view points, after the page list's
/// own padding.
struct ViewportSize: Equatable {
    let width: Double
    let height: Double
}

extension ViewerStore {
    /// The rungs zoom in/out step through — Windows' ladder, limited to this
    /// store's `minimumZoom...maximumZoom`.
    static let zoomLadder = [0.10, 0.25, 0.50, 0.75, 1.00, 1.25, 1.50, 2.00, 3.00, 4.00, 6.00, 8.00]
        .filter { (minimumZoom...maximumZoom).contains($0) }

    /// The factor `page` is shown at. A fit mode falls back to the explicit
    /// factor until the viewport has been measured, rather than collapsing
    /// the page.
    func zoom(forPage page: Int) -> Double {
        guard zoomMode != .custom, pageSlots.indices.contains(page), let viewport else { return zoom }
        let dimensions = pageSlots[page].dimensions
        guard dimensions.width > 0, dimensions.height > 0, viewport.width > 0 else { return zoom }
        let widthFactor = viewport.width / dimensions.width
        let factor: Double
        switch zoomMode {
        case .custom: return zoom
        case .fitWidth: factor = widthFactor
        case .fitPage:
            guard viewport.height > 0 else { return zoom }
            factor = min(widthFactor, viewport.height / dimensions.height)
        }
        return Self.clampZoom(factor)
    }

    /// The factor a zoom readout shows and zoom in/out steps from: the
    /// current page's, since in a fit mode pages of different sizes resolve
    /// differently.
    var displayedZoom: Double {
        zoom(forPage: currentPage)
    }

    func setZoomMode(_ mode: ZoomMode) {
        guard mode != .custom else { return setZoom(displayedZoom) }
        zoomMode = mode
    }

    /// Called whenever the page list's visible area changes size. Only a fit
    /// mode depends on it; an explicit zoom must not move.
    func setViewport(_ size: ViewportSize) {
        guard size != viewport else { return }
        viewport = size
    }

    /// Steps to the next ladder rung above (`direction > 0`) or below the
    /// factor currently shown, leaving any fit mode.
    func stepZoom(_ direction: Int) {
        let current = displayedZoom
        let epsilon = 1e-9
        let next = direction > 0
            ? Self.zoomLadder.first { $0 > current + epsilon } ?? Self.maximumZoom
            : Self.zoomLadder.last { $0 < current - epsilon } ?? Self.minimumZoom
        setZoom(next)
    }

    static func clampZoom(_ value: Double) -> Double {
        guard value.isFinite else { return 1.0 }
        return min(max(value, minimumZoom), maximumZoom)
    }
}
