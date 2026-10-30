// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Edits of several content items as one undo step, through the generated
/// UniFFI functions: text blocks (B25 T-207) and multi-selections (T-208) —
/// Windows' `PdfDocumentFacade.TextBlocks.cs` and `.ContentSelection.cs`.
extension UniFfiPdfCoreClient {
    /// Applies `edit` when it is a group edit; `false` for any other.
    func applyGroupEdit(handle: DocumentHandle, edit: ContentEdit) throws -> Bool {
        switch edit {
        case let .moveTextBlock(block, dx, dy):
            try mapTextEditErrors(handle) { try Vitela.moveTextBlock(handle: handle, runs: block.runs.map(FfiContentTextRun.init), dx: dx, dy: dy) }
        case let .removeTextBlock(block):
            try mapTextEditErrors(handle) { try Vitela.removeTextBlock(handle: handle, runs: block.runs.map(FfiContentTextRun.init)) }
        case let .retypeTextBlock(block, text):
            try mapTextEditErrors(handle) {
                try Vitela.retypeTextBlock(handle: handle, runs: block.runs.map(FfiContentTextRun.init), text: text,
                                           withInsertedFont: block.substitutesFont)
            }
        case let .moveSelection(selection, dx, dy):
            try mapTextEditErrors(handle) { try Vitela.moveContentSelection(handle: handle, selection: FfiContentSelection(selection), dx: dx, dy: dy) }
        case let .removeSelection(selection):
            try mapTextEditErrors(handle) { try Vitela.removeContentSelection(handle: handle, selection: FfiContentSelection(selection)) }
        case let .pasteContent(page, clip, left, top):
            try mapTextEditErrors(handle) {
                try Vitela.pasteContent(handle: handle, page: UInt32(page), clip: FfiContentClip(clip), left: left, top: top)
            }
        default:
            return false
        }
        return true
    }
}

extension UniFfiPdfCoreClient {
    /// Copying reads the file, never the session: the core refuses a member
    /// with an unsaved edit, and extraction permission gates it.
    func copyContent(document: any PdfDocument, selection: ContentSelection) throws -> ContentClip {
        let handle = try handle(of: document)
        do {
            let clip = try Vitela.copyContentSelection(handle: handle, selection: FfiContentSelection(selection))
            return ContentClip(pdf: clip.pdf, bbox: PdfRect(x: clip.bbox.x, y: clip.bbox.y, width: clip.bbox.width, height: clip.bbox.height))
        } catch FfiError.UnsupportedOperation {
            throw EditFailure.notPermitted(ContentEditing.copyRefused)
        } catch let error as FfiError {
            throw EditFailure(error)
        }
    }
}

private extension FfiContentClip {
    init(_ clip: ContentClip) {
        self.init(pdf: clip.pdf, bbox: FfiRect(x: clip.bbox.x, y: clip.bbox.y, width: clip.bbox.width, height: clip.bbox.height))
    }
}

private extension FfiContentSelection {
    /// A text block is sent as its runs.
    init(_ selection: ContentSelection) {
        self.init(runs: selection.blocks.flatMap(\.runs).map(FfiContentTextRun.init),
                  images: selection.images.map(FfiContentImageItem.init),
                  paths: selection.paths.map(FfiContentPathItem.init))
    }
}
