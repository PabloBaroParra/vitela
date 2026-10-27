import AppKit
import SwiftUI
import XCTest
@testable import Vitela

/// The page list scrolls both ways, like Linux's canvas `ScrolledWindow` and
/// Windows' `PageScroller`: a page zoomed wider than the window pans
/// sideways, and one that fits stays centred with nothing to pan.
final class PageScrollTests: XCTestCase {
    func testRailPersistsAndHomeHidesEditorCommandsInNarrowAndWideWindows() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))
        model.showHome()
        let layouts: [(CGFloat, ColorScheme)] = [(640, .light), (1100, .light), (640, .dark), (1100, .dark)]
        for (width, scheme) in layouts {
            let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: width, height: 700),
                                  styleMask: [.titled], backing: .buffered, defer: false)
            window.isReleasedWhenClosed = false
            let host = NSHostingView(rootView: ViewerRootView(model: model).environment(\.colorScheme, scheme))
            host.appearance = NSAppearance(named: scheme == .dark ? .darkAqua : .aqua)
            window.contentView = host
            window.orderFront(nil)
            defer { window.close() }
            model.showHome()
            // Inspect the actual AppKit scroll views and text fields. SwiftUI's
            // virtual AX nodes are not all materialized in a hosted unit test.
            // Home must get the whole workspace height: leaving even a single
            // toolbar above it fails this, without relying on a copied flag.
            waitUntil({ self.scrollViews(host).contains { $0.frame.width > 300 && abs($0.frame.height - host.frame.height) < 1 } },
                      "Home did not get the full workspace height")
            let railWidth = width < AppRail.collapseBelowWidth ? AppRail.compactWidth : AppRail.expandedWidth
            XCTAssertTrue(scrollViews(host).contains { abs($0.frame.width - railWidth) < 1 }, "the persistent rail was not laid out")
            attach(host, named: "Home-\(Int(width))pt-\(scheme)")
            XCTAssertFalse(hasDocumentSearch(host), "Home must not carry the editor chrome")

            model.returnToDocument()
            waitUntil({ self.hasDocumentSearch(host) }, "Return to Document did not restore commands")
            attach(host, named: "Editor-\(Int(width))pt-\(scheme)")
            XCTAssertTrue(scrollViews(host).contains { abs($0.frame.width - railWidth) < 1 }, "the rail must also stay beside the document")
            XCTAssertEqual(host.frame.width, width, accuracy: 1, "commands must not widen the window")
        }
    }

    private func scrollViews(_ view: NSView) -> [NSScrollView] {
        ((view as? NSScrollView).map { [$0] } ?? []) + view.subviews.flatMap(scrollViews)
    }

    private func hasDocumentSearch(_ view: NSView) -> Bool {
        (view as? NSTextField)?.placeholderString == "Find in document" || view.subviews.contains(where: hasDocumentSearch)
    }

    private func attach(_ view: NSView, named name: String) {
        guard let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return }
        view.cacheDisplay(in: view.bounds, to: bitmap)
        let image = NSImage(size: view.bounds.size)
        image.addRepresentation(bitmap)
        let attachment = XCTAttachment(image: image)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    /// Linux's FlowBox and Windows' ToolbarPanel wrap instead of making the
    /// window as wide as every command put together. Exercise the actual
    /// SwiftUI/AppKit layout, not a second copy of its sizing arithmetic.
    func testEditorToolbarWrapsInANarrowWindow() {
        let model = ViewerViewModel(store: ViewerStore(client: FakePdfCoreClient(pages: [])))
        let toolbar = ViewerToolbar(model: model, showsProperties: .constant(false),
                                    showsComments: .constant(false), showsForm: .constant(false))
        assertWraps(toolbar)
    }

    func testAnnotationToolbarWrapsInANarrowWindow() {
        assertWraps(AnnotationToolbar(store: ViewerStore(client: FakePdfCoreClient(pages: []))))
    }

    func testSelectedImageControlsAlsoWrap() {
        let store = ViewerStore(client: FakePdfCoreClient(pages: []))
        store.contentEdit.selectedImage = ContentImage(id: 1, page: 0,
            bbox: PdfRect(x: 10, y: 10, width: 100, height: 100), resourceXObjectName: "Im1")
        let model = ViewerViewModel(store: store)
        assertWraps(ViewerToolbar(model: model, showsProperties: .constant(false),
                                 showsComments: .constant(false), showsForm: .constant(false)))
    }

    func testSelectedAnnotationControlsAlsoWrap() {
        let store = ViewerStore(client: FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)]))
        store.open(bytes: Data([1]))
        store.applyAnnotationEdit(.add(kind: .shape, page: 0,
            rect: PdfRect(x: 100, y: 100, width: 100, height: 50), color: .annotationDefault))
        XCTAssertTrue(store.beginAnnotationGesture(page: 0, at: PdfPoint(x: 150, y: 120), zoom: 1))
        store.endAnnotationGesture(at: PdfPoint(x: 150, y: 120))
        XCTAssertNotNil(store.selectedAnnotation)
        assertWraps(AnnotationToolbar(store: store))
    }

    private func assertWraps<Content: View>(_ toolbar: Content, file: StaticString = #filePath, line: UInt = #line) {
        func height(at width: CGFloat) -> CGFloat {
            let host = NSHostingView(rootView: toolbar.frame(width: width).fixedSize(horizontal: false, vertical: true)
                .environment(\.colorScheme, .light).background(Color.white))
            host.appearance = NSAppearance(named: .aqua)
            host.frame.size = CGSize(width: width, height: 500)
            host.layoutSubtreeIfNeeded()
            attach(host, named: "\(String(describing: Content.self))-\(Int(width))pt")
            return host.fittingSize.height
        }
        let narrow = height(at: 640)
        let wide = height(at: 1280)
        XCTAssertGreaterThan(narrow, wide + 20, "commands must wrap rather than overflow", file: file, line: line)
        XCTAssertLessThan(narrow, 260, "leave room for the document in a 480pt-high window", file: file, line: line)
    }

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
        guard let pages = scrollViews(window.contentView!).max(by: {
            ($0.documentView?.frame.height ?? 0) < ($1.documentView?.frame.height ?? 0)
        }), let document = pages.documentView else {
            XCTFail("no page scroller")
            return (0, 0)
        }
        return (Double(pages.contentView.bounds.width), Double(document.frame.width))
    }
}
