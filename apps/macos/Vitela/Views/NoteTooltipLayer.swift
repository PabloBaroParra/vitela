import AppKit
import SwiftUI

/// Hangs a native tooltip on each note's area of a page. An AppKit layer
/// because SwiftUI's `.help` needs the view under the pointer, and the page's
/// gesture overlay must keep every press: this view registers tooltip rects
/// but takes no click (`hitTest` is `nil`), so selection, drags and tools see
/// the page exactly as before — Windows' transparent note rectangle, whose
/// press still bubbles to the canvas.
struct NoteTooltipLayer: NSViewRepresentable {
    let tooltips: [NoteTooltip]

    func makeNSView(context: Context) -> TooltipView { TooltipView() }

    func updateNSView(_ view: TooltipView, context: Context) {
        view.show(tooltips)
    }

    final class TooltipView: NSView {
        /// Retained here: a tooltip rect does not retain the string it shows.
        private var texts: [NSString] = []

        override var isFlipped: Bool { true }

        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        func show(_ tooltips: [NoteTooltip]) {
            removeAllToolTips()
            texts = tooltips.map { $0.text as NSString }
            for (tooltip, text) in zip(tooltips, texts) {
                let frame = NSRect(x: tooltip.frame.left, y: tooltip.frame.top, width: max(tooltip.frame.width, 1), height: max(tooltip.frame.height, 1))
                // An owner that does not answer `stringForToolTip` is asked
                // for its description: the string itself.
                addToolTip(frame, owner: text, userData: nil)
            }
        }
    }
}
