import AppKit
import XCTest
@testable import Vitela

final class NoteTooltipLayerTests: XCTestCase {
    /// The layer sits over the page's gesture overlay: it must never take a
    /// press, or selecting, dragging and placing would stop under a note.
    func testTheLayerTakesNoClick() {
        let view = NoteTooltipLayer.TooltipView(frame: NSRect(x: 0, y: 0, width: 200, height: 200))
        view.show([NoteTooltip(frame: PlacedRect(left: 10, top: 10, width: 50, height: 50), text: "Note")])

        XCTAssertNil(view.hitTest(NSPoint(x: 20, y: 20)))
        XCTAssertTrue(view.isFlipped, "rects are placed top-left, like the page")
    }
}
