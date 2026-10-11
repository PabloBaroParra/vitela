// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Editing and saving through the generated UniFFI API. Kept apart from the
/// read-only viewer calls in `UniFfiPdfCoreClient.swift` so each file stays
/// about one boundary.
extension UniFfiPdfCoreClient {
    func documentInfo(document: any PdfDocument) throws -> DocumentInfo {
        DocumentInfo(try handle(of: document).readDocumentInfo())
    }

    func setDocumentInfo(document: any PdfDocument, info: DocumentInfo) throws {
        let handle = try handle(of: document)
        try Self.mapEditErrors {
            try applyEdit(handle: handle, command: .setDocumentInfo(after: FfiDocumentInfo(info)))
        }
    }

    func contentEditingAllowed(document: any PdfDocument) -> Bool {
        (try? handle(of: document).contentEditingAllowed()) ?? false
    }

    func undo(document: any PdfDocument) -> Bool {
        guard let handle = try? handle(of: document) else { return false }
        return Vitela.undo(handle: handle)
    }

    func redo(document: any PdfDocument) -> Bool {
        guard let handle = try? handle(of: document) else { return false }
        return Vitela.redo(handle: handle)
    }

    func canUndo(document: any PdfDocument) -> Bool {
        (try? handle(of: document).canUndo()) ?? false
    }

    func canRedo(document: any PdfDocument) -> Bool {
        (try? handle(of: document).canRedo()) ?? false
    }

    func saveWillInvalidateSignatures(document: any PdfDocument) throws -> Bool {
        let handle = try handle(of: document)
        return try Self.mapEditErrors { try willInvalidateSignatures(handle: handle, intent: .default) }
    }

    func save(document: any PdfDocument, acknowledgingSignatureLoss: Bool) throws -> Data {
        let handle = try handle(of: document)
        return try Self.mapEditErrors {
            try saveToBytes(
                handle: handle,
                intent: .default,
                signatures: acknowledgingSignatureLoss ? .proceedAndInvalidate : .unacknowledged
            )
        }
    }

    func applyPageEdit(document: any PdfDocument, edit: PageEdit) throws {
        let handle = try handle(of: document)
        do {
            try applyEdit(handle: handle, command: FfiEditCommand(edit))
        } catch FfiError.UnsupportedOperation {
            // Named, as on Windows: the generic "not supported" reads like a
            // missing feature, when it is the document's own permissions (or
            // an encryption that cannot survive the rewrite) that said no.
            if case .insertBlank = edit {
                throw EditFailure.notPermitted("This document does not allow adding a blank page.")
            }
            throw EditFailure.notPermitted("This document does not allow its pages to be rearranged, rotated or removed.")
        } catch let error as FfiError {
            throw EditFailure(error)
        }
    }

    func refreshPreview(document: any PdfDocument) throws {
        let handle = try handle(of: document)
        try Self.mapEditErrors { try Vitela.refreshPreview(handle: handle) }
    }

    func pageDimensions(document: any PdfDocument) -> [PageDimensions] {
        guard let handle = try? handle(of: document) else { return document.pages }
        return handle.pageDimensions().map(PageDimensions.init)
    }

    func handle(of document: any PdfDocument) throws -> DocumentHandle {
        guard let document = document as? UniFfiDocument else {
            throw EditFailure.failed("document is not backed by UniFFI")
        }
        return document.handle
    }

    /// Turns the generated error enum into `EditFailure`, keeping the core's
    /// own detail text: `String(describing:)` on a generated error prints the
    /// case name and its fields, which is no use to a reader.
    static func mapEditErrors<T>(_ body: () throws -> T) throws -> T {
        do {
            return try body()
        } catch let error as FfiError {
            throw EditFailure(error)
        }
    }
}

extension EditFailure {
    init(_ error: FfiError) {
        switch error {
        case .SignaturesWouldBeInvalidated:
            self = .signaturesWouldBeInvalidated
        case let .UnsupportedOperation(detail):
            self = .notPermitted(Self.sentence(detail))
        case let .InvalidSaveRequest(detail), let .InvalidImage(detail), let .InvalidPageSelection(detail),
             let .RenderFailed(detail), let .Io(detail), let .Internal(detail):
            self = .failed(Self.sentence(detail))
        default:
            self = .failed(String(describing: error))
        }
    }

    /// The core writes details as lower-case clauses ("annotation editing is
    /// not permitted"); a status line reads better as a sentence.
    /// Not `private`: refusals read straight from the core
    /// (`compression_refusal`, `signing_refusal`, `import_refusal`) are
    /// written the same way.
    static func sentence(_ detail: String) -> String {
        guard let first = detail.first else { return detail }
        let capitalized = first.uppercased() + detail.dropFirst()
        return capitalized.hasSuffix(".") ? capitalized : capitalized + "."
    }
}

private extension FfiEditCommand {
    /// Single-page forms of the range commands Windows' facade issues
    /// (`PdfDocumentFacade.Organize.cs`), so both shells record the same
    /// history steps. Positions were range-checked by the store.
    init(_ edit: PageEdit) {
        switch edit {
        case let .rotate(page, degrees):
            self = .rotatePages(from: UInt32(page), count: 1, deltaDegrees: Int32(degrees))
        case let .remove(page):
            self = .removePages(index: UInt32(page), count: 1)
        case let .insertBlank(at, orientation):
            self = .insertBlankPage(
                index: UInt32(at),
                size: .a4,
                orientation: orientation == .portrait ? .portrait : .landscape
            )
        case let .move(from, to):
            self = .movePages(from: UInt32(from), count: 1, to: UInt32(to))
        }
    }
}

private extension DocumentInfo {
    init(_ info: FfiDocumentInfo) {
        self.init(
            title: info.title,
            author: info.author,
            subject: info.subject,
            keywords: info.keywords,
            creator: info.creator,
            producer: info.producer,
            creationDate: info.creationDate.map(MetadataDate.init),
            modDate: info.modDate.map(MetadataDate.init)
        )
    }
}

private extension FfiDocumentInfo {
    init(_ info: DocumentInfo) {
        self.init(
            title: info.title,
            author: info.author,
            subject: info.subject,
            keywords: info.keywords,
            creator: info.creator,
            producer: info.producer,
            creationDate: info.creationDate.map(FfiPdfDate.init),
            modDate: info.modDate.map(FfiPdfDate.init)
        )
    }
}

private extension MetadataDate {
    init(_ date: FfiPdfDate) {
        let offset: Offset
        switch date.offset {
        case .utc: offset = .utc
        case let .plus(hours, minutes): offset = .plus(hours: Int(hours), minutes: Int(minutes))
        case let .minus(hours, minutes): offset = .minus(hours: Int(hours), minutes: Int(minutes))
        }
        self.init(
            year: Int(date.year), month: Int(date.month), day: Int(date.day),
            hour: Int(date.hour), minute: Int(date.minute), second: Int(date.second),
            offset: offset
        )
    }
}

private extension FfiPdfDate {
    /// `MetadataDateText.parse` already range-checked every component, so
    /// the narrowing conversions here cannot trap.
    init(_ date: MetadataDate) {
        let offset: FfiPdfDateOffset
        switch date.offset {
        case .utc: offset = .utc
        case let .plus(hours, minutes): offset = .plus(hours: UInt8(hours), minutes: UInt8(minutes))
        case let .minus(hours, minutes): offset = .minus(hours: UInt8(hours), minutes: UInt8(minutes))
        }
        self.init(
            year: UInt16(date.year), month: UInt8(date.month), day: UInt8(date.day),
            hour: UInt8(date.hour), minute: UInt8(date.minute), second: UInt8(date.second),
            offset: offset
        )
    }
}

/// Protecting through the generated UniFFI API — Windows' facade
/// `ProtectToDestinationAsync` makes the same call.
extension UniFfiPdfCoreClient {
    func protectionWillInvalidateSignatures(document: any PdfDocument) throws -> Bool {
        let handle = try handle(of: document)
        return try Self.mapEditErrors { try Vitela.protectionWillInvalidateSignatures(handle: handle) }
    }

    func protect(
        document: any PdfDocument, openPassword: String, permissionsPassword: String, acknowledgingSignatureLoss: Bool
    ) throws -> Data {
        let handle = try handle(of: document)
        return try Self.mapEditErrors {
            try protectToBytes(
                handle: handle,
                openPassword: openPassword,
                permissionsPassword: permissionsPassword,
                signatures: acknowledgingSignatureLoss ? .proceedAndInvalidate : .unacknowledged
            )
        }
    }
}
