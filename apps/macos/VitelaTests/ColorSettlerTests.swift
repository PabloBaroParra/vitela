import XCTest
@testable import Vitela

final class ColorSettlerTests: XCTestCase {
    private var scheduled: [DispatchWorkItem] = []
    private var commits: [(UInt64, PdfColor)] = []

    private func settler() -> ColorSettler {
        ColorSettler(schedule: { [unowned self] _, work in scheduled.append(work) })
    }

    private func record(_ id: UInt64, _ color: PdfColor) { commits.append((id, color)) }

    private let red = PdfColor(r: 255, g: 0, b: 0)
    private let green = PdfColor(r: 0, g: 255, b: 0)
    private let blue = PdfColor(r: 0, g: 0, b: 255)

    func testADragAcrossThePanelCommitsOnlyTheColorItSettlesOn() {
        let settler = settler()
        for color in [red, green, blue] { settler.pick(color, for: 7, commit: record) }
        XCTAssertTrue(commits.isEmpty, "nothing lands while the panel is moving")
        XCTAssertEqual(settler.color(for: 7), blue)

        scheduled.forEach { $0.perform() }

        XCTAssertEqual(commits.map(\.0), [7])
        XCTAssertEqual(commits.map(\.1), [blue], "one recolor, so one undo step")
        XCTAssertNil(settler.color(for: 7))
    }

    func testPickingForAnotherAnnotationLandsTheFirstOnItsOwnAnnotation() {
        let settler = settler()
        settler.pick(red, for: 7, commit: record)
        settler.pick(blue, for: 8, commit: record)

        XCTAssertEqual(commits.map(\.0), [7])
        XCTAssertEqual(commits.map(\.1), [red])
        XCTAssertEqual(settler.color(for: 8), blue)
    }

    func testFlushLandsAHeldColorAtOnceAndOnlyOnce() {
        let settler = settler()
        settler.pick(red, for: 7, commit: record)
        settler.flush()
        scheduled.forEach { $0.perform() }
        settler.flush()

        XCTAssertEqual(commits.map(\.1), [red])
    }
}
