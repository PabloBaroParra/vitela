// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// The document's text-extraction permission, held at every door the text
/// leaves by: search, loading a page's characters for a selection, and copy
/// — Linux's `text_extraction_refusal` and Windows'
/// `TextExtractionRefusalMessage`. The core refuses the work itself; this
/// says why, up front, before anything is queued or a match count could
/// show what the document withholds.
extension ViewerStore {
    static let textExtractionRefusalMessage = "This document does not permit copying or extracting its text."

    /// `nil` when the open document lets its text be extracted, or none is open.
    var textExtractionRefusal: String? {
        guard let document, !client.textExtractionAllowed(document: document) else { return nil }
        return Self.textExtractionRefusalMessage
    }
}
