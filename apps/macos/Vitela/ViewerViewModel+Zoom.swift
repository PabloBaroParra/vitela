import AppKit
import Foundation

/// Feeds the page list's size to the store's fit modes
/// (`ViewerStore+Zoom.swift`).
extension ViewerViewModel {
    /// The page list's padding on each side, which a fitted page must leave.
    static let pageListPadding = 16.0

    /// How long a resize must pause before the fitted pages re-render. Every
    /// size a live resize passes through would otherwise queue a render of
    /// each visible page on the serial render queue.
    static let viewportSettleDelay = 0.15

    /// Reports the page list's size. The first report applies at once, so a
    /// document opens fitted; later ones wait for the resize to settle.
    func updateViewport(_ size: CGSize) {
        let viewport = Self.pageViewport(for: size)
        pendingViewport?.cancel()
        guard store.viewport != nil else {
            store.setViewport(viewport)
            return
        }
        let update = DispatchWorkItem { [weak self] in self?.store.setViewport(viewport) }
        pendingViewport = update
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.viewportSettleDelay, execute: update)
    }

    /// The area a page may fill: the scroll view less its padding and, with
    /// always-visible scrollers, the vertical scroller.
    static func pageViewport(for size: CGSize) -> ViewportSize {
        let scroller = NSScroller.preferredScrollerStyle == .legacy
            ? Double(NSScroller.scrollerWidth(for: .regular, scrollerStyle: .legacy))
            : 0
        return ViewportSize(
            width: max(Double(size.width) - 2 * pageListPadding - scroller, 0),
            height: max(Double(size.height) - 2 * pageListPadding, 0)
        )
    }
}
