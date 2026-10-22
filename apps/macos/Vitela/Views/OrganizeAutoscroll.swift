import AppKit
import SwiftUI

/// The edge ramp Organize scrolls by while a card is dragged — Linux's
/// `organize/autoscroll.rs` and Windows' `Viewer/DragEdgeScroll.cs`, same
/// numbers: a hot zone a third of the viewport deep, capped at 56pt, moving
/// 2pt per tick at its inner edge up to 16pt at the viewport's edge.
enum DragEdgeScroll {
    static let edge = 56.0
    static let minStep = 2.0
    static let maxStep = 16.0

    /// What one tick should move the view, given the pointer `y` points from
    /// the viewport's top in a viewport `height` tall: negative towards the
    /// start of the document, positive towards its end, zero between the zones.
    static func step(y: Double, height: Double) -> Double {
        let edge = min(Self.edge, height / 3)
        guard edge > 0 else { return 0 }
        let depth = y < edge ? edge - y : y > height - edge ? y - (height - edge) : 0
        guard depth > 0 else { return 0 }
        let speed = minStep + (maxStep - minStep) * min(max(depth / edge, 0), 1)
        return y < edge ? -speed : speed
    }

    /// Moves `scrollView` `step` points towards the end of its document
    /// (negative: towards the start), clamped to the content it has, and
    /// reports whether anything moved.
    @discardableResult
    static func scroll(_ scrollView: NSScrollView, by step: Double) -> Bool {
        guard step != 0, let document = scrollView.documentView else { return false }
        let clip = scrollView.contentView
        let range = max(document.frame.height - clip.bounds.height, 0)
        // A flipped document grows downwards from y = 0; an unflipped one
        // keeps its start at the top of its frame.
        let current = document.isFlipped ? clip.bounds.origin.y : range - clip.bounds.origin.y
        let target = min(max(current + step, 0), range)
        guard abs(target - current) > 0.01 else { return false }
        clip.scroll(to: NSPoint(x: clip.bounds.origin.x, y: document.isFlipped ? target : range - target))
        scrollView.reflectScrolledClipView(clip)
        return true
    }
}

/// Put behind an Organize scroll view's content: while one of its cards is
/// being dragged, holding the pointer near the top or bottom edge scrolls the
/// view, so a destination off screen when the drag began can still be
/// reached. SwiftUI's `ScrollView` does not do this for its drop targets, and
/// a drag holds the pointer, so neither the scroller nor the wheel can help.
struct OrganizeAutoscroll: NSViewRepresentable {
    /// Whether a card from this view is being dragged right now.
    let isDragging: () -> Bool

    func makeNSView(context: Context) -> AutoscrollView {
        AutoscrollView(isDragging: isDragging)
    }

    func updateNSView(_ view: AutoscrollView, context: Context) {
        view.isDragging = isDragging
    }

    final class AutoscrollView: NSView {
        var isDragging: () -> Bool
        private var timer: Timer?
        private var ticking = false

        init(isDragging: @escaping () -> Bool) {
            self.isDragging = isDragging
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { nil }

        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            schedule(ticking: false)
        }

        /// A slow check while no drag is running, a 60Hz one while it is:
        /// the drag session gives a view that is not its destination no
        /// event to start from, and polling every frame for the whole time
        /// Organize is open would cost a wakeup per frame for nothing.
        private func schedule(ticking: Bool) {
            timer?.invalidate()
            timer = nil
            self.ticking = ticking
            guard window != nil else { return }
            let timer = Timer(timeInterval: ticking ? 1.0 / 60 : 0.2, repeats: true) { [weak self] _ in self?.tick() }
            RunLoop.main.add(timer, forMode: .common)
            self.timer = timer
        }

        private func tick() {
            guard let window, let scrollView = enclosingScrollView,
                  NSEvent.pressedMouseButtons & 1 != 0, isDragging()
            else { return ticking ? schedule(ticking: false) : () }
            if !ticking { return schedule(ticking: true) }
            let point = scrollView.convert(window.mouseLocationOutsideOfEventStream, from: nil)
            let height = Double(scrollView.bounds.height)
            guard point.x >= 0, point.x <= scrollView.bounds.width else { return }
            let y = scrollView.isFlipped ? Double(point.y) : height - Double(point.y)
            DragEdgeScroll.scroll(scrollView, by: DragEdgeScroll.step(y: y, height: height))
        }

        deinit { timer?.invalidate() }
    }
}
