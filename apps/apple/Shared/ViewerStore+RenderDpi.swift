// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// The DPI a page is rasterized at for the reader — Linux's
/// `layout.rs` `resolve_page_box`: the zoom times the display's backing
/// scale, so a Retina screen gets one bitmap pixel per screen pixel instead
/// of a 1× bitmap stretched to twice its size, held inside the same bounds.
extension ViewerStore {
    /// Linux's `MIN_RENDER_DPI` and `MAX_RENDER_DPI`.
    static let renderDpiRange = 24.0...600.0
    /// Linux's `MAX_BASE_PIXELS`: past it the page bitmap is rendered
    /// coarser rather than larger, and the visible part is tiled instead.
    static let maxPagePixels = 8_000_000.0

    /// The screen's pixels per point; a value that is no scale counts as 1×.
    /// A change leaves every slot stale, so pages re-render when next shown.
    func setRenderScale(_ scale: Double) {
        renderScale = scale.isFinite && scale > 0 ? scale : 1
    }

    /// The DPI the screen asks for, before the page bitmap's pixel budget:
    /// where it is higher than `dpi(forPage:zoom:)`, the part of the page in
    /// view is drawn again from tiles at this DPI (`ViewerStore+Tiles.swift`).
    func intendedDpi(zoom: Double) -> Double {
        (Self.baseDpi * zoom * renderScale).rounded(.down).clamped(to: Self.renderDpiRange)
    }

    func dpi(forPage page: Int, zoom: Double) -> Int {
        let desired = intendedDpi(zoom: zoom)
        guard pageSlots.indices.contains(page) else { return Int(desired) }
        let size = pageSlots[page].dimensions
        let pixels = size.width * desired / Self.baseDpi * (size.height * desired / Self.baseDpi)
        guard pixels.isFinite, pixels > Self.maxPagePixels else { return Int(desired) }
        return Int(max((desired * (Self.maxPagePixels / pixels).squareRoot()).rounded(.down), Self.renderDpiRange.lowerBound))
    }

    /// Whether `slot` already shows, or is fetching, this zoom at this DPI.
    func isUpToDate(_ slot: PageSlot, at pageZoom: Double, dpi: Int) -> Bool {
        guard slot.renderZoom == pageZoom, slot.renderDpi == dpi else { return false }
        switch slot.status {
        case .rendering, .rendered, .failed: return true
        case .idle: return false
        }
    }
}

private extension Double {
    func clamped(to range: ClosedRange<Double>) -> Double { Swift.min(Swift.max(self, range.lowerBound), range.upperBound) }
}
