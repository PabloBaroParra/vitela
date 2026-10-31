import XCTest
@testable import Vitela

final class RecentDocumentsTests: XCTestCase {
    private var calendar: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        return calendar
    }()
    /// 2026-03-10 15:30 UTC.
    private let now = Date(timeIntervalSince1970: 1_773_156_600)

    private func doc(_ path: String, hoursAgo: Double) -> RecentDocument {
        RecentDocument(path: path, openedAt: now.addingTimeInterval(-hoursAgo * 3600))
    }

    func testRecordingMovesADocumentToTheFrontOnce() {
        let list = [doc("/a.pdf", hoursAgo: 1), doc("/b.pdf", hoursAgo: 2)]

        let recorded = RecentDocuments.recording("/b.pdf", at: now, in: list)

        XCTAssertEqual(recorded.map(\.path), ["/b.pdf", "/a.pdf"])
        XCTAssertEqual(recorded.first?.openedAt, now)
    }

    func testRecordingKeepsTheBookmarkUnlessANewOneIsGiven() throws {
        let old = Data([1]), new = Data([2])
        let list = [RecentDocument(path: "/a.pdf", openedAt: now.addingTimeInterval(-60), bookmark: old)]

        XCTAssertEqual(RecentDocuments.recording("/a.pdf", at: now, in: list).first?.bookmark, old)
        XCTAssertEqual(RecentDocuments.recording("/a.pdf", at: now, in: list, bookmark: new).first?.bookmark, new)
        XCTAssertNil(RecentDocuments.recording("/b.pdf", at: now, in: list).first?.bookmark)
    }

    func testAListWrittenBeforeBookmarksStillDecodes() throws {
        let json = Data(#"[{"path":"/a.pdf","openedAt":0}]"#.utf8)

        let decoded = try JSONDecoder().decode([RecentDocument].self, from: json)

        XCTAssertEqual(decoded, [RecentDocument(path: "/a.pdf", openedAt: Date(timeIntervalSinceReferenceDate: 0))])
    }

    func testRecordingStandardizesThePathAndKeepsTheCapacity() {
        let list = (0..<RecentDocuments.capacity).map { doc("/d\($0).pdf", hoursAgo: Double($0)) }

        let recorded = RecentDocuments.recording("/x/../new.pdf", at: now, in: list)

        XCTAssertEqual(recorded.count, RecentDocuments.capacity)
        XCTAssertEqual(recorded.first?.path, "/new.pdf")
        XCTAssertFalse(recorded.contains { $0.path == "/d\(RecentDocuments.capacity - 1).pdf" }, "the oldest is dropped")
    }

    func testFirstShownIsTheNewestCardTheSearchLeaves() {
        let list = [doc("/old-report.pdf", hoursAgo: 50), doc("/a.pdf", hoursAgo: 1), doc("/report.pdf", hoursAgo: 3)]

        XCTAssertEqual(RecentDocuments.firstShown(list, now: now, calendar: calendar), "/a.pdf")
        XCTAssertEqual(RecentDocuments.firstShown(list, matching: "report", now: now, calendar: calendar), "/report.pdf")
        XCTAssertNil(RecentDocuments.firstShown(list, matching: "none", now: now, calendar: calendar))
        XCTAssertNil(RecentDocuments.firstShown([], now: now, calendar: calendar))
    }

    func testRemovingTakesOnlyThatDocument() {
        let list = [doc("/a.pdf", hoursAgo: 1), doc("/b.pdf", hoursAgo: 2)]
        XCTAssertEqual(RecentDocuments.removing("/a.pdf", from: list).map(\.path), ["/b.pdf"])
    }

    func testDocumentsAreGroupedByCalendarDayNewestFirst() {
        let list = [
            doc("/old.pdf", hoursAgo: 72),
            doc("/morning.pdf", hoursAgo: 6),
            doc("/last-night.pdf", hoursAgo: 20),
            doc("/now.pdf", hoursAgo: 0.5),
        ]

        let groups = RecentDocuments.grouped(list, now: now, calendar: calendar)

        XCTAssertEqual(groups.map(\.bucket), [.today, .yesterday, .earlier])
        XCTAssertEqual(groups[0].documents.map(\.name), ["now.pdf", "morning.pdf"])
        XCTAssertEqual(groups[1].documents.map(\.name), ["last-night.pdf"])
        XCTAssertEqual(groups[2].documents.map(\.name), ["old.pdf"])
    }

    func testTheSearchMatchesNamesIgnoringCaseAndDropsEmptyGroups() {
        let list = [doc("/Reports/Invoice.pdf", hoursAgo: 1), doc("/notes.pdf", hoursAgo: 48)]

        let groups = RecentDocuments.grouped(list, matching: " invo ", now: now, calendar: calendar)

        XCTAssertEqual(groups.map(\.bucket), [.today])
        XCTAssertEqual(groups[0].documents.map(\.name), ["Invoice.pdf"])
        XCTAssertTrue(RecentDocuments.grouped(list, matching: "Reports", now: now, calendar: calendar).isEmpty, "the folder is not the name")
    }

    func testCaptionsSayWhenAndHowManyPages() {
        let locale = Locale(identifier: "en_US_POSIX")
        // Newer ICU puts a narrow no-break space before AM/PM.
        func caption(_ document: RecentDocument, _ pages: Int?) -> String {
            RecentDocuments.caption(document, pageCount: pages, now: now, calendar: calendar, locale: locale)
                .replacingOccurrences(of: "\u{202F}", with: " ")
        }
        XCTAssertEqual(
            caption(doc("/a.pdf", hoursAgo: 1), nil),
            "Opened today, 2:30 PM"
        )
        XCTAssertEqual(
            caption(doc("/a.pdf", hoursAgo: 20), 1),
            "Opened yesterday, 7:30 PM · 1 page"
        )
        XCTAssertEqual(
            caption(doc("/a.pdf", hoursAgo: 72), 12),
            "Opened 3/7/26 · 12 pages"
        )
    }

    func testEmptyMessagesFollowWindows() {
        XCTAssertEqual(RecentDocuments.emptyMessage(listIsEmpty: true), "No recent documents yet. Open a PDF and it will show up here.")
        XCTAssertEqual(RecentDocuments.emptyMessage(listIsEmpty: false), "No recent document matches that search.")
    }
}
