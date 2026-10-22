// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// One document the reader opened, and when.
struct RecentDocument: Equatable, Codable {
    let path: String
    let openedAt: Date

    var url: URL { URL(fileURLWithPath: path) }
    var name: String { url.lastPathComponent }
}

/// The rules behind Home's Recent list, as Linux (`home/recents.rs`) and
/// Windows (`MainWindow.Recents.cs`) apply them: newest first, grouped into
/// Today, Yesterday and Earlier, filtered by name, and each card captioned
/// with when it was opened. Storage is the shell's.
enum RecentDocuments {
    /// How many documents the list keeps.
    static let capacity = 30

    /// `list` with `path` opened at `date`: moved to the front, never twice,
    /// and the oldest dropped past `capacity`.
    static func recording(_ path: String, at date: Date, in list: [RecentDocument]) -> [RecentDocument] {
        let standardized = URL(fileURLWithPath: path).standardizedFileURL.path
        let rest = list.filter { $0.path != standardized }
        return Array(([RecentDocument(path: standardized, openedAt: date)] + rest).prefix(capacity))
    }

    /// `list` without `path`.
    static func removing(_ path: String, from list: [RecentDocument]) -> [RecentDocument] {
        let standardized = URL(fileURLWithPath: path).standardizedFileURL.path
        return list.filter { $0.path != standardized }
    }

    enum Bucket: String, CaseIterable {
        case today = "Today"
        case yesterday = "Yesterday"
        case earlier = "Earlier"
    }

    /// Windows' `RecentBucket`, by calendar day in `calendar`'s zone.
    static func bucket(of date: Date, now: Date, calendar: Calendar = .current) -> Bucket {
        if calendar.isDate(date, inSameDayAs: now) { return .today }
        if let yesterday = calendar.date(byAdding: .day, value: -1, to: now), calendar.isDate(date, inSameDayAs: yesterday) {
            return .yesterday
        }
        return .earlier
    }

    /// The documents whose name contains `query` (ignoring case), newest
    /// first, grouped by day; empty groups are left out.
    static func grouped(
        _ list: [RecentDocument], matching query: String = "", now: Date, calendar: Calendar = .current
    ) -> [(bucket: Bucket, documents: [RecentDocument])] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
        let shown = list
            .filter { needle.isEmpty || $0.name.localizedCaseInsensitiveContains(needle) }
            .sorted { $0.openedAt > $1.openedAt }
        return Bucket.allCases.compactMap { bucket in
            let documents = shown.filter { self.bucket(of: $0.openedAt, now: now, calendar: calendar) == bucket }
            return documents.isEmpty ? nil : (bucket, documents)
        }
    }

    /// The card the keyboard moves to once the focused card is removed: the
    /// newest one still shown under `query` — Linux's `focus_first_card` and
    /// Windows' `FocusFirstRecentCard`. `nil` when no card is left.
    static func firstShown(
        _ list: [RecentDocument], matching query: String = "", now: Date, calendar: Calendar = .current
    ) -> String? {
        grouped(list, matching: query, now: now, calendar: calendar).first?.documents.first?.path
    }

    /// "Opened today, 9:41 AM", "Opened yesterday, …", or "Opened 3/2/26" —
    /// Windows' card caption, with a page count once the preview knows it.
    static func caption(
        _ document: RecentDocument, pageCount: Int?, now: Date, calendar: Calendar = .current, locale: Locale = .current
    ) -> String {
        let bucket = bucket(of: document.openedAt, now: now, calendar: calendar)
        let formatter = DateFormatter()
        formatter.locale = locale
        formatter.timeZone = calendar.timeZone
        let opened: String
        if bucket == .earlier {
            formatter.dateStyle = .short
            formatter.timeStyle = .none
            opened = "Opened \(formatter.string(from: document.openedAt))"
        } else {
            formatter.dateStyle = .none
            formatter.timeStyle = .short
            opened = "Opened \(bucket.rawValue.lowercased()), \(formatter.string(from: document.openedAt))"
        }
        guard let pageCount else { return opened }
        return "\(opened) · \(pageCount == 1 ? "1 page" : "\(pageCount) pages")"
    }

    /// Windows' empty-list wording.
    static func emptyMessage(listIsEmpty: Bool) -> String {
        listIsEmpty
            ? "No recent documents yet. Open a PDF and it will show up here."
            : "No recent document matches that search."
    }
}
