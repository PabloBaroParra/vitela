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

    func imageReplacementRefusal(document: any PdfDocument, image: ContentImage) -> String? {
        guard let handle = try? handle(of: document) else { return "The document is no longer open." }
        guard (try? handle.imageSourceBytes(item: FfiContentImageItem(image))) != nil else { return Self.replaceRefusal(handle) }
        return nil
    }

    func textRunHasPendingMove(document: any PdfDocument, run: ContentTextRun) -> Bool {
        guard let handle = try? handle(of: document) else { return false }
        return (try? handle.textRunHasPendingMove(page: UInt32(run.page), id: run.id)) ?? false
    }

    func applyContentEdit(document: any PdfDocument, edit: ContentEdit) throws {
        let handle = try handle(of: document)
        guard let command = FfiEditCommand(edit) else {
            switch edit {
            case let .insertText(page, rect, text):
                return try insertText(handle: handle, page: page, rect: rect, text: text)
            case let .replaceImage(image, bytes):
                return try replaceImage(handle: handle, image: image, with: bytes)
            case let .insertImage(page, rect, bytes):
                return try insertImage(handle: handle, page: page, rect: rect, bytes: bytes)
            default:
                return
            }
        }
        do {
            try Vitela.applyEdit(handle: handle, command: command)
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

private extension UniFfiPdfCoreClient {
    /// A new line under a font resource name the page has never declared — a
    /// fresh one also keeps queued insertions apart — as Windows and Android
    /// choose it.
    func insertText(handle: DocumentHandle, page: Int, rect: PdfRect, text: String) throws {
        do {
            let declared = try handle.pageFontFamilies(page: UInt32(page))
            var fontName: String
            repeat {
                fontName = "VitelaText" + UUID().uuidString.replacingOccurrences(of: "-", with: "")
            } while declared[fontName] != nil
            let run = FfiContentTextRun(id: 0, page: UInt32(page), bbox: FfiRect(rect), resourceFontName: fontName, fontKind: .standard14, text: text)
            try Vitela.applyEdit(handle: handle, command: .insertTextRun(item: run))
        } catch let FfiError.EncodingGap(character, _) {
            throw EditFailure.failed("Helvetica cannot show \"\(character)\". Try different characters.")
        } catch FfiError.UnsupportedOperation {
            throw EditFailure.notPermitted(handle.contentEditingAllowed() ? "This text cannot be added here." : ContentEditing.forbidden)
        } catch let error as FfiError {
            throw EditFailure(error)
        }
    }
}

private extension UniFfiPdfCoreClient {
    /// `before` is read here, at apply time, not carried from the check: an
    /// edit may have landed in between, and the undo must restore what the
    /// page holds now — Android's reasoning.
    func replaceImage(handle: DocumentHandle, image: ContentImage, with bytes: Data) throws {
        let item = FfiContentImageItem(image)
        do {
            try Vitela.applyEdit(handle: handle, command: .replaceImageSource(item: item, before: handle.imageSourceBytes(item: item), after: bytes))
        } catch FfiError.InvalidImage {
            throw EditFailure.failed(ContentEditing.notAnImage)
        } catch FfiError.UnsupportedOperation {
            throw EditFailure.notPermitted(Self.replaceRefusal(handle))
        } catch let error as FfiError {
            throw EditFailure(error)
        }
    }

    /// A new image under an XObject name of its own: the core refuses one the
    /// page already has rather than overwrite it, and a random name keeps
    /// queued insertions apart — Android's choice.
    func insertImage(handle: DocumentHandle, page: Int, rect: PdfRect, bytes: Data) throws {
        let name = "VitelaImage" + UUID().uuidString.replacingOccurrences(of: "-", with: "")
        let item = FfiContentImageItem(id: 0, page: UInt32(page), bbox: FfiRect(rect), resourceXobjectName: name)
        do {
            try Vitela.applyEdit(handle: handle, command: .insertImage(item: item, source: bytes))
        } catch FfiError.InvalidImage {
            throw EditFailure.failed(ContentEditing.notAnImage)
        } catch FfiError.UnsupportedOperation {
            throw EditFailure.notPermitted(handle.contentEditingAllowed() ? "This image cannot be added here." : ContentEditing.forbidden)
        } catch let error as FfiError {
            throw EditFailure(error)
        }
    }

    static func replaceRefusal(_ handle: DocumentHandle) -> String {
        handle.contentEditingAllowed() ? ContentEditing.replaceRefused : ContentEditing.forbidden
    }
}

private extension FfiEditCommand {
    /// `nil` for an insertion, which needs the page's fonts to name its own.
    init?(_ edit: ContentEdit) {
        switch edit {
        case let .retypeText(run, text):
            self = run.substitutesFont
                ? .replaceTextRunWithInsertedFont(item: FfiContentTextRun(run), after: text)
                : .replaceTextRunContent(item: FfiContentTextRun(run), after: text)
        case let .removeText(run):
            self = .removeTextRun(item: FfiContentTextRun(run))
        case let .moveText(run, rect):
            self = .moveTextRun(item: FfiContentTextRun(run), to: FfiRect(rect))
        case .insertText, .replaceImage, .insertImage:
            return nil
        case let .moveImage(image, rect):
            self = .moveImage(item: FfiContentImageItem(image), to: FfiRect(rect))
        case let .resizeImage(image, rect):
            self = .resizeImage(item: FfiContentImageItem(image), to: FfiRect(rect))
        case let .removeImage(image):
            // The core keeps the original for undo itself, as on Windows.
            self = .removeImage(item: FfiContentImageItem(image), source: nil)
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

private extension FfiContentImageItem {
    init(_ image: ContentImage) {
        self.init(id: image.id, page: UInt32(image.page), bbox: FfiRect(image.bbox), resourceXobjectName: image.resourceXObjectName)
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
