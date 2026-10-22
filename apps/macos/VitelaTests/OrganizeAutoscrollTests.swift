import AppKit
import SwiftUI
import XCTest
@testable import Vitela

/// The same cases as Linux's `organize/autoscroll.rs` tests.
final class OrganizeAutoscrollTests: XCTestCase {
    private let height = 600.0

    func testTheMiddleOfTheViewportDoesNotScroll() {
        for y in [56.0, 300, 544] {
            XCTAssertEqual(DragEdgeScroll.step(y: y, height: height), 0, "y = \(y)")
        }
    }

    func testEachEdgeScrollsTowardsTheContentThePointerIsReachingFor() {
        XCTAssertLessThan(DragEdgeScroll.step(y: 10, height: height), 0)
        XCTAssertGreaterThan(DragEdgeScroll.step(y: 590, height: height), 0)
    }

    func testTheDeeperIntoAnEdgeThePointerIsTheFasterItScrolls() {
        let shallow = DragEdgeScroll.step(y: 50, height: height)
        let deep = DragEdgeScroll.step(y: 5, height: height)
        XCTAssertLessThan(abs(shallow), abs(deep))
        XCTAssertEqual(DragEdgeScroll.step(y: 0, height: height), -DragEdgeScroll.maxStep)
        XCTAssertEqual(DragEdgeScroll.step(y: height, height: height), DragEdgeScroll.maxStep)
    }

    func testAPointerPastAnEdgeScrollsNoFasterThanOneRestingOnIt() {
        XCTAssertEqual(DragEdgeScroll.step(y: -200, height: height), -DragEdgeScroll.maxStep)
        XCTAssertEqual(DragEdgeScroll.step(y: height + 200, height: height), DragEdgeScroll.maxStep)
    }

    func testAViewportTooShortForTwoFullHotZonesKeepsANeutralMiddle() {
        XCTAssertEqual(DragEdgeScroll.step(y: 45, height: 90), 0)
        XCTAssertLessThan(DragEdgeScroll.step(y: 10, height: 90), 0)
        XCTAssertGreaterThan(DragEdgeScroll.step(y: 80, height: 90), 0)
    }

    func testAViewportWithNoHeightYetNeverScrolls() {
        XCTAssertEqual(DragEdgeScroll.step(y: 0, height: 0), 0)
    }

    private final class FlippedView: NSView {
        override var isFlipped: Bool { true }
    }

    private func scroller(flipped: Bool) -> NSScrollView {
        let scrollView = NSScrollView(frame: NSRect(x: 0, y: 0, width: 200, height: 100))
        let document = flipped ? FlippedView() : NSView()
        document.frame = NSRect(x: 0, y: 0, width: 200, height: 1000)
        scrollView.documentView = document
        if !flipped { document.scroll(NSPoint(x: 0, y: 900)) }
        return scrollView
    }

    func testAStepMovesTheViewTowardsTheEndOfTheDocument() {
        let scrollView = scroller(flipped: true)

        XCTAssertTrue(DragEdgeScroll.scroll(scrollView, by: 16))

        XCTAssertEqual(scrollView.contentView.bounds.origin.y, 16)
    }

    func testAStepStopsAtTheEndOfTheContentItIsWalkingTowards() {
        let scrollView = scroller(flipped: true)

        XCTAssertFalse(DragEdgeScroll.scroll(scrollView, by: -16), "already at the start")
        XCTAssertTrue(DragEdgeScroll.scroll(scrollView, by: 5000))
        XCTAssertEqual(scrollView.contentView.bounds.origin.y, 900)
        XCTAssertFalse(DragEdgeScroll.scroll(scrollView, by: 16), "already at the end")
    }

    func testAnUnflippedDocumentStartsAtItsTop() {
        let scrollView = scroller(flipped: false)

        XCTAssertTrue(DragEdgeScroll.scroll(scrollView, by: 16))

        XCTAssertEqual(scrollView.contentView.bounds.origin.y, 884, "down the page is towards y = 0")
    }

    func testANeutralStepMovesNothingAndReportsIt() {
        let scrollView = scroller(flipped: true)

        XCTAssertFalse(DragEdgeScroll.scroll(scrollView, by: 0))
        XCTAssertEqual(scrollView.contentView.bounds.origin.y, 0)
    }

    func testBehindAScrollViewsContentItReachesTheScrollerItDrives() throws {
        let content = ScrollView {
            Color.clear.frame(width: 200, height: 1000)
                .background(OrganizeAutoscroll { false })
        }
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 200, height: 100), styleMask: .borderless, backing: .buffered, defer: true)
        window.contentView = NSHostingView(rootView: content)
        window.contentView?.layoutSubtreeIfNeeded()

        let view = try XCTUnwrap(Self.find(OrganizeAutoscroll.AutoscrollView.self, in: window.contentView))
        let scrollView = try XCTUnwrap(view.enclosingScrollView, "SwiftUI's ScrollView is an NSScrollView on macOS")
        XCTAssertTrue(DragEdgeScroll.scroll(scrollView, by: 16))
    }

    private static func find<V: NSView>(_ type: V.Type, in view: NSView?) -> V? {
        guard let view else { return nil }
        if let match = view as? V { return match }
        return view.subviews.lazy.compactMap { find(type, in: $0) }.first
    }
}
