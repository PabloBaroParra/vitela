// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Reading and editing what a page paints, through the generated UniFFI API
/// — the calls Windows' facade (`ReadPageContentAsync`, `ReplaceTextRunAsync`)
/// and Android's `GeneratedPdfCore` make.
extension UniFfiPdfCoreClient {
    func readPageContent(document: any PdfDocument, page: Int) throws -> PageContent {
        let handle = try handle(of: document)
        let content = try Self.mapEditErrors { try handle.readPageContent(page: UInt32(page)) }
        return PageContent(textRuns: content.textRuns.map(ContentTextRun.init), images: content.images.map(ContentImage.init))
    }

    func applyContentEdit(document: any PdfDocument, edit: ContentEdit) throws {
        let handle = try handle(of: document)
        do {
            try Vitela.applyEdit(handle: handle, command: FfiEditCommand(edit))
        } catch let FfiError.EncodingGap(character, _) {
            // Named, with the character in it: the one failure the reader can
            // clear by typing something else — Windows' and Android's wording.
            throw EditFailure.failed("This text's font cannot show \"\(character)\". Try different characters.")
        } catch FfiError.UnsupportedOperation {
            // The /P bit, or the editor declining this run or its stream; the
            // core's own sentence is a developer's, not a reader's.
            throw EditFailure.notPermitted(handle.contentEditingAllowed()
                ? "This text cannot be edited."
                : ContentEditing.forbidden)
        } catch let error as FfiError {
            throw EditFailure(error)
        }
    }
}

private extension FfiEditCommand {
    init(_ edit: ContentEdit) {
        switch edit {
        case let .retypeText(run, text):
            self = run.substitutesFont
                ? .replaceTextRunWithInsertedFont(item: FfiContentTextRun(run), after: text)
                : .replaceTextRunContent(item: FfiContentTextRun(run), after: text)
        case let .removeText(run):
            self = .removeTextRun(item: FfiContentTextRun(run))
        }
    }
}

private extension ContentTextRun {
    init(_ run: FfiContentTextRun) {
        self.init(
            id: run.id, page: Int(run.page), bbox: PdfRect(run.bbox), resourceFontName: run.resourceFontName,
            fontKind: ContentFontKind(run.fontKind), text: run.text
        )
    }
}

private extension FfiContentTextRun {
    init(_ run: ContentTextRun) {
        self.init(
            id: run.id, page: UInt32(run.page), bbox: FfiRect(run.bbox), resourceFontName: run.resourceFontName,
            fontKind: FfiFontKind(run.fontKind), text: run.text
        )
    }
}

private extension ContentImage {
    init(_ image: FfiContentImageItem) {
        self.init(id: image.id, page: Int(image.page), bbox: PdfRect(image.bbox), resourceXObjectName: image.resourceXobjectName)
    }
}

private extension ContentFontKind {
    init(_ kind: FfiFontKind) {
        switch kind {
        case .standard14: self = .standard14
        case .embeddedSimple: self = .embeddedSimple
        case .embeddedComposite: self = .embeddedComposite
        }
    }
}

private extension FfiFontKind {
    init(_ kind: ContentFontKind) {
        switch kind {
        case .standard14: self = .standard14
        case .embeddedSimple: self = .embeddedSimple
        case .embeddedComposite: self = .embeddedComposite
        }
    }
}

private extension PdfRect {
    init(_ rect: FfiRect) {
        self.init(x: rect.x, y: rect.y, width: rect.width, height: rect.height)
    }
}

private extension FfiRect {
    init(_ rect: PdfRect) {
        self.init(x: rect.x, y: rect.y, width: rect.width, height: rect.height)
    }
}
