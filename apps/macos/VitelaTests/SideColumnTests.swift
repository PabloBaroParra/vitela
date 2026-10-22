import XCTest
@testable import Vitela

/// The divider rule Windows' `SideColumn` and Linux's `side_panel.rs` share.
final class SideColumnTests: XCTestCase {
    private func column() -> SideColumn { SideColumn(minimum: 120, maximum: 320, initial: 160) }

    func testTheInitialWidthIsKeptInsideTheLimits() {
        XCTAssertEqual(SideColumn(minimum: 120, maximum: 320, initial: 60).preferredWidth, 120)
        XCTAssertEqual(SideColumn(minimum: 120, maximum: 320, initial: 900).preferredWidth, 320)
    }

    func testDraggingWidensAndNarrowsUpToTheMaximum() {
        var column = column()
        column.beginDrag(isOpen: true)
        XCTAssertTrue(column.drag(by: 40))
        XCTAssertEqual(column.preferredWidth, 200)
        XCTAssertTrue(column.drag(by: -20), "a drag is measured from where it began")
        XCTAssertEqual(column.preferredWidth, 140)
        XCTAssertTrue(column.drag(by: 500))
        XCTAssertEqual(column.preferredWidth, 320)
    }

    func testDraggingPastTheMinimumFoldsAndKeepsTheLastOpenWidth() {
        var column = column()
        column.beginDrag(isOpen: true)
        XCTAssertTrue(column.drag(by: 20))
        XCTAssertFalse(column.drag(by: -50))
        XCTAssertEqual(column.preferredWidth, 180, "the toggle reopens at the last open width")
    }

    func testAFoldedColumnUnfoldsOnceDraggedBackAcrossItsMinimum() {
        var column = column()
        column.beginDrag(isOpen: false)
        XCTAssertFalse(column.drag(by: 100))
        XCTAssertTrue(column.drag(by: 130))
        XCTAssertEqual(column.preferredWidth, 130)
    }

    func testOneKeyStepMovesTheDividerSixteenPoints() {
        var column = column()
        column.beginDrag(isOpen: true)
        XCTAssertTrue(column.drag(by: SideColumn.keyStep))
        XCTAssertEqual(column.preferredWidth, 176)
    }
}
