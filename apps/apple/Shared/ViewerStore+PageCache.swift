// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Which page bitmaps the reader keeps — Linux's `render.rs` `CACHE_PAGES`
/// and Windows dropping a slot's image outside its `PageWindow`. A page
/// bitmap can be up to 8 MP (`ViewerStore+RenderDpi.swift`); kept for every
/// page ever shown, a long document read end to end would hold them all.
extension ViewerStore {
    /// Pages kept rendered on each side of the ones laid out.
    static let cachedPageRadius = 3

    /// Called with the spans of the page rows the scroll view has laid out:
    /// every page further than `cachedPageRadius` from them gives up its
    /// bitmap and renders again when it comes back.
    func releasePages(farFrom spans: [Int: PageViewportSpan]) {
        let shown = spans.keys.filter { pageSlots.indices.contains($0) }
        guard let first = shown.min(), let last = shown.max() else { return }
        let kept = max(first - Self.cachedPageRadius, 0)...min(last + Self.cachedPageRadius, pageSlots.count - 1)
        for index in pageSlots.indices where !kept.contains(index) {
            // Only slots holding something: each write publishes, and this
            // runs on every scroll step.
            guard pageSlots[index].image != nil || pageSlots[index].status != .idle else { continue }
            pageSlots[index].image = nil
            pageSlots[index].status = .idle
            pageSlots[index].renderZoom = nil
        }
    }
}
