import AppKit
import SwiftUI
import XCTest
@testable import Vitela

/// The page list scrolls both ways, like Linux's canvas `ScrolledWindow` and
/// Windows' `PageScroller`: a page zoomed wider than the window pans
/// sideways, and one that fits stays centred with nothing to pan.
final class PageScrollTests: XCTestCase {
    func testAPageWiderThanTheWindowScrollsSidewaysAndOneThatFitsDoesNot() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client), sampleLoader: { Data([1]) })
        model.openSample()
        waitUntil({ model.store.state == .loaded }, "the sample never opened")
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 700),
                              styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = NSHostingView(rootView: ViewerRootView(model: model))
        defer { window.close() }

        model.store.setZoom(3)
        let zoomed = pageScroller(in: window)
        XCTAssertGreaterThan(zoomed.document, zoomed.clip + 1000, "a 300% page must reach past the window's sides")

        model.store.setZoom(1)
        let fitting = pageScroller(in: window)
        XCTAssertEqual(fitting.document, fitting.clip, accuracy: 1, "a page that fits must leave nothing to pan")
    }

    /// The widths of the scroll view that holds the pages, after layout.
    private func pageScroller(in window: NSWindow) -> (clip: Double, document: Double) {
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        func scrollViews(_ view: NSView) -> [NSScrollView] {
            ((view as? NSScrollView).map { [$0] } ?? []) + view.subviews.flatMap(scrollViews)
        }
        guard let pages = scrollViews(window.contentView!).max(by: {
            ($0.documentView?.frame.height ?? 0) < ($1.documentView?.frame.height ?? 0)
        }), let document = pages.documentView else {
            XCTFail("no page scroller")
            return (0, 0)
        }
        return (Double(pages.contentView.bounds.width), Double(document.frame.width))
    }
}
