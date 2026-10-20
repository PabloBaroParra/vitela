// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// What a file handed to the app from outside (a drop, Finder's Open With)
/// is for — the twin of Windows' `Viewer/FileDropRouting.cs`. Decided once,
/// from the content signature rather than the file name, so a PDF saved
/// without its extension still opens and a renamed image is not handed to
/// the PDF parser. Content is not validated here: the core's own open still
/// reports a corrupt file that merely starts like a PDF.
enum DroppedFileKind: Equatable {
    /// A PDF to open, replacing whatever is on screen.
    case document
    /// A PNG or JPEG, which a drop on a page places there as a stamp.
    case image
    /// Nothing this shell can open.
    case unsupported

    private static let pdfSignature = Data("%PDF-".utf8)

    static func classify(_ bytes: Data) -> DroppedFileKind {
        if bytes.starts(with: pdfSignature) { return .document }
        return ViewerStore.isSupportedStampImage(bytes) ? .image : .unsupported
    }

    /// The one file of a drop that is acted on. Dragging several files at
    /// once is usually an accident — a whole selection swept in — so, as on
    /// Windows, exactly the first wins and the rest are ignored.
    static func firstFile(_ urls: [URL]) -> URL? {
        urls.first { $0.isFileURL }
    }
}
