// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// The editable text form of a metadata date, `YYYY-MM-DD HH:MM:SS` — the
/// same format the Windows shell's `MetadataDateText` and the GTK shell's
/// properties panel use, so a date reads the same on every platform. Like
/// theirs, it checks each component's range but not the calendar (no
/// "February 30" rule): the PDF date format itself does not either.
enum MetadataDateText {
    struct ParseError: Error, Equatable, LocalizedError {
        let reason: String
        var errorDescription: String? { reason }
    }

    static func format(_ date: MetadataDate?) -> String {
        guard let date else { return "" }
        return String(
            format: "%04d-%02d-%02d %02d:%02d:%02d",
            date.year, date.month, date.day, date.hour, date.minute, date.second
        )
    }

    /// `nil` for blank input, which clears the date. `offset` is the zone the
    /// previous value carried: the text form has none, so editing a date
    /// keeps the one it had.
    static func parse(_ raw: String, offset: MetadataDate.Offset) throws -> MetadataDate? {
        let text = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if text.isEmpty { return nil }
        // Whitespace separates date and time, never individual components.
        let split = text.firstIndex(where: \.isWhitespace) ?? text.endIndex
        let dateParts = text[..<split].split(separator: "-", omittingEmptySubsequences: false)
        guard dateParts.count == 3 else { throw ParseError(reason: "expected YYYY-MM-DD") }
        let year = try component(dateParts[0], 0...Int(UInt16.max), "year")
        let month = try component(dateParts[1], 1...12, "month")
        let day = try component(dateParts[2], 1...31, "day")

        let timeText = text[split...].trimmingCharacters(in: .whitespacesAndNewlines)
        let time = timeText.isEmpty ? [] : timeText.split(separator: ":", omittingEmptySubsequences: false)
        guard time.isEmpty || time.count == 2 || time.count == 3 else {
            throw ParseError(reason: "expected HH:MM or HH:MM:SS")
        }
        return MetadataDate(
            year: year, month: month, day: day,
            hour: time.isEmpty ? 0 : try component(time[0], 0...23, "hour"),
            minute: time.isEmpty ? 0 : try component(time[1], 0...59, "minute"),
            second: time.count < 3 ? 0 : try component(time[2], 0...59, "second"),
            offset: offset
        )
    }

    private static func component<S: StringProtocol>(_ text: S, _ range: ClosedRange<Int>, _ name: String) throws -> Int {
        let digits = text.hasPrefix("+") ? text.dropFirst() : text[...]
        guard !digits.isEmpty, digits.allSatisfy(\.isASCII), digits.allSatisfy(\.isNumber), let value = Int(digits) else {
            throw ParseError(reason: "\(name) is not a number")
        }
        guard range.contains(value) else {
            throw ParseError(reason: "\(name) \(value) is out of range (\(range.lowerBound)-\(range.upperBound))")
        }
        return value
    }
}
