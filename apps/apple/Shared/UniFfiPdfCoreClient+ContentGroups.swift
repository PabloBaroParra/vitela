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
        default:
            return false
        }
        return true
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
